//! `http` (capability `net_http`, contract 0.3.0, RC-100-33): one request from a secret source to
//! the loopback port its manifest grants.
//!
//! Adapted from rDownloader's `crates/rd-plugin-host/src/component/host_impls.rs`
//! (`controlled_http_request`) at commit 26c8592a6: the target checked against the manifest
//! before anything leaves, no redirect followed (a redirect is the answer), the answer capped,
//! the time the service took credited back to the plugin. Not taken: domains, value templates,
//! cookies, the account's secrets - a loopback bridge such as `bw serve` needs none of them, and
//! the first plugin that does brings them with its own checks (`docs/plugins.md`, "Capabilities
//! and rights").

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::{Duration, Instant},
};

use rc_plugin_types::{Failure, FailureKind};
use reqwest::{Method, header};
use url::{Host, Url};
use zeroize::Zeroizing;

use crate::{
    bindings::{rcommander::plugin::types, secrets_world::rcommander::plugin::http},
    failure::{self, codes},
    host_impls::to_wit_failure,
    runtime::PluginStoreState,
};

/// The methods a plugin may send; anything else is refused before the request is built.
const METHODS: [&str; 6] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"];
/// The most headers one request may carry, and their size together.
const MAX_HEADERS: usize = 64;
const MAX_HEADER_BYTES: usize = 64 * 1024;
/// The largest body one request may carry (1 MiB, a window).
const MAX_BODY_BYTES: usize = 1024 * 1024;
/// The most answer headers handed back; the rest are dropped.
const MAX_ANSWER_HEADERS: usize = 128;
/// Headers the host sets itself: a plugin naming one could smuggle a second request or another
/// target past the check.
const RESERVED_HEADERS: [&str; 6] = [
    "host",
    "connection",
    "content-length",
    "transfer-encoding",
    "upgrade",
    "proxy-authorization",
];

/// What one store of a secret source may reach: the loopback port its manifest grants.
#[derive(Clone)]
pub(crate) struct HttpGrant {
    port: u16,
    max_response_bytes: u64,
    client: reqwest::Client,
}

impl HttpGrant {
    /// The grant for `port`; `timeout` bounds one request. A port no plugin may reach
    /// (rCommander's own MCP port) gets no grant at all: the secret source is built without one
    /// (`SecretsPlugin::new`), so its every request is denied.
    ///
    /// # Errors
    ///
    /// `plugin_host_failed` when the client cannot be built.
    pub(crate) fn new(
        port: u16,
        max_response_bytes: u64,
        timeout: Duration,
    ) -> Result<Self, Failure> {
        // `localhost` is this machine's loopback, never what a hosts file or DNS make of it.
        let loopback = [
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port),
        ];
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(timeout)
            .resolve_to_addrs("localhost", &loopback)
            .build()
            .map_err(failure::host_failed)?;
        Ok(Self {
            port,
            max_response_bytes,
            client,
        })
    }

    /// `url` when it is the granted target: `http`, a loopback host, the granted port, no user.
    fn target(&self, url: &str) -> Result<Url, Failure> {
        let denied = || failure::coded(FailureKind::AccessDenied, codes::PLUGIN_HTTP_DENIED);
        let url = Url::parse(url).map_err(|_| denied())?;
        let loopback = match url.host() {
            Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
            Some(Host::Ipv4(address)) => address == Ipv4Addr::LOCALHOST,
            Some(Host::Ipv6(address)) => address == Ipv6Addr::LOCALHOST,
            None => false,
        };
        let port = url.port_or_known_default();
        if url.scheme() != "http"
            || !loopback
            || port != Some(self.port)
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(denied());
        }
        Ok(url)
    }

    /// Sends one request and reads its answer, at most `max_response_bytes` of it.
    pub(crate) async fn request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Vec<u8>,
    ) -> Result<http::HttpResponse, Failure> {
        let url = self.target(url)?;
        let too_large = || failure::coded(FailureKind::BadData, codes::PLUGIN_REQUEST_TOO_LARGE);
        let method = METHODS
            .iter()
            .find(|known| **known == method)
            .and_then(|known| Method::from_bytes(known.as_bytes()).ok())
            .ok_or_else(|| failure::coded(FailureKind::BadData, codes::PLUGIN_BAD_OUTPUT))?;
        let header_bytes: usize = headers
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum();
        if headers.len() > MAX_HEADERS
            || header_bytes > MAX_HEADER_BYTES
            || body.len() > MAX_BODY_BYTES
        {
            return Err(too_large());
        }
        let mut map = header::HeaderMap::new();
        for (name, value) in headers {
            let bad = || failure::coded(FailureKind::BadData, codes::PLUGIN_BAD_OUTPUT);
            let name = header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| bad())?;
            if RESERVED_HEADERS.contains(&name.as_str()) {
                return Err(bad());
            }
            map.append(
                name,
                header::HeaderValue::from_str(value).map_err(|_| bad())?,
            );
        }
        let failed = |_| failure::coded(FailureKind::Io, codes::PLUGIN_HTTP_FAILED);
        let mut response = self
            .client
            .request(method, url)
            .headers(map)
            .body(body)
            .send()
            .await
            .map_err(failed)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| Some((name.to_string(), value.to_str().ok()?.to_owned())))
            .take(MAX_ANSWER_HEADERS)
            .collect();
        // The answer may be a secret: wiped when it is refused, and every buffer it outgrows.
        let mut body = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            if (body.len() + chunk.len()) as u64 > self.max_response_bytes {
                return Err(failure::coded(
                    FailureKind::BadData,
                    codes::PLUGIN_HTTP_TOO_LARGE,
                ));
            }
            append(&mut body, &chunk);
        }
        Ok(http::HttpResponse {
            status,
            headers,
            body: std::mem::take(&mut *body),
        })
    }
}

/// Appends `chunk` to `body`. A buffer too small is copied into a larger one, and the old one is
/// wiped as it is dropped - a `Vec` that grows by itself leaves its old bytes behind.
fn append(body: &mut Zeroizing<Vec<u8>>, chunk: &[u8]) {
    if body.capacity() - body.len() < chunk.len() {
        let wanted = (body.len() + chunk.len()).max(body.capacity() * 2);
        let mut larger = Zeroizing::new(Vec::with_capacity(wanted));
        larger.extend_from_slice(body);
        *body = larger;
    }
    body.extend_from_slice(chunk);
}

/// A store without the grant has no `http` linked at all, unless its port is one no plugin
/// reaches: then this denies every request (and if the linker and the store ever disagree).
impl http::Host for PluginStoreState {
    async fn request(
        &mut self,
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<http::HttpResponse, types::Failure> {
        let Some(grant) = self.http.clone() else {
            return Err(to_wit_failure(failure::coded(
                FailureKind::AccessDenied,
                codes::PLUGIN_HTTP_DENIED,
            )));
        };
        // The service is waiting, not the plugin thinking (rDownloader's RA-HOST-05).
        let started = Instant::now();
        let answer = grant.request(&method, &url, &headers, body).await;
        self.credit_host_time(started.elapsed());
        answer.map_err(to_wit_failure)
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
