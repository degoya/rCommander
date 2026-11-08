//! `http` (capability `net_http`, contract 0.3.0, RC-100-33): one request from a secret source to
//! the loopback port its manifest grants, or (contract 0.4.0, RC-100-39) from an inventory to the
//! address the person entered for its source, with `{{secret:<slot>}}` markers filled in
//! (`markers.rs`).
//!
//! Adapted from rDownloader's `crates/rd-plugin-host/src/component/host_impls.rs`
//! (`controlled_http_request`) at commit 26c8592a6: the target checked against the grant before
//! anything leaves, no redirect followed (a redirect is the answer), the answer capped, the time
//! the service took credited back to the plugin. Not taken: domains, value templates, cookies -
//! a loopback bridge such as `bw serve` and an inventory's REST API need none of them, and the
//! first plugin that does brings them with its own checks (`docs/plugins.md`, "Capabilities and
//! rights"). The credit is bounded (RC-080-69): one request at most its time limit, one call at
//! most `runtime::HTTP_TIME_BUDGET` in all.

use std::{
    borrow::Cow,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use rc_plugin_types::{Failure, FailureKind};
use reqwest::{Method, header};
use secrecy::SecretString;
use url::{Host, Origin, Url};
use zeroize::Zeroizing;

use crate::{
    bindings::{rcommander::plugin::types, secrets_world::rcommander::plugin::http},
    failure::{self, codes},
    host_impls::to_wit_failure,
    runtime::PluginStoreState,
};

mod markers;

use markers::Slots;

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

/// Where one store's requests may go.
#[derive(Clone)]
enum Target {
    /// A secret source's loopback port (RC-100-33): plain HTTP to this machine.
    Loopback(u16),
    /// The address the person entered for an inventory source (RC-100-39): a relative address
    /// is resolved against it, and every request has to keep its scheme, host and port.
    Entered { base: Url, origin: Origin },
}

/// What one store may reach: a secret source's loopback port, or an inventory's entered address
/// with the values its markers are filled with.
#[derive(Clone)]
pub(crate) struct HttpGrant {
    target: Target,
    max_response_bytes: u64,
    /// One request's time limit: the most of its time credited back.
    timeout: Duration,
    client: reqwest::Client,
    /// The values of `{{secret:<slot>}}`; none for a secret source.
    secrets: Slots,
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
            target: Target::Loopback(port),
            max_response_bytes,
            timeout,
            client,
            secrets: Arc::from(Vec::new()),
        })
    }

    /// The grant of an inventory source (RC-100-39): requests to the scheme, host and port of
    /// `address`, the address the person entered and confirmed, its markers filled from
    /// `secrets`. HTTPS, or plain HTTP to this machine's loopback (an inventory in a local
    /// container) - never a token in clear over the network. `localhost` is this machine's
    /// loopback, as for a secret source.
    ///
    /// `tls` is the engine's TLS for the address (RC-100-40): the system roots, then the
    /// certificate the person trusted for its host and port (RC-070-02), so a self-signed one
    /// (a Proxmox node) passes only once a person trusted it. Without it the system roots alone
    /// decide; verification is never turned off.
    ///
    /// # Errors
    ///
    /// `plugin_http_denied` for any other address, `plugin_host_failed` when the client cannot
    /// be built.
    pub(crate) fn entered(
        address: &Url,
        secrets: Vec<(String, SecretString)>,
        tls: Option<rustls::ClientConfig>,
        max_response_bytes: u64,
        timeout: Duration,
    ) -> Result<Self, Failure> {
        if !entered_address_allowed(address) {
            return Err(failure::coded(
                FailureKind::AccessDenied,
                codes::PLUGIN_HTTP_DENIED,
            ));
        }
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout);
        if is_loopback(address) {
            let port = address.port_or_known_default().unwrap_or(80);
            let loopback = [
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
                SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port),
            ];
            builder = builder.no_proxy().resolve_to_addrs("localhost", &loopback);
        }
        if let Some(tls) = tls.filter(|_| address.scheme() == "https") {
            builder = builder.tls_backend_preconfigured(tls);
        }
        let client = builder.build().map_err(failure::host_failed)?;
        Ok(Self {
            target: Target::Entered {
                base: address.clone(),
                origin: address.origin(),
            },
            max_response_bytes,
            timeout,
            client,
            secrets: Arc::from(secrets),
        })
    }

    /// `text` with every value a marker may be filled with masked: for the plugin's log lines,
    /// its failures and its answers.
    pub(crate) fn mask<'a>(&self, text: &'a str) -> Cow<'a, str> {
        markers::mask(text, &self.secrets)
    }

    /// `url` when it is the granted target, without a user: for a secret source `http`, a
    /// loopback host and the granted port; for an inventory the entered address's scheme, host
    /// and port. An inventory may name a relative address (`/api/dcim/devices/`), resolved
    /// against the entered one: it never needs to know where its server is.
    fn target(&self, url: &str) -> Result<Url, Failure> {
        let denied = || failure::coded(FailureKind::AccessDenied, codes::PLUGIN_HTTP_DENIED);
        let url = match (Url::parse(url), &self.target) {
            (Ok(url), _) => url,
            (Err(url::ParseError::RelativeUrlWithoutBase), Target::Entered { base, .. }) => {
                base.join(url).map_err(|_| denied())?
            }
            (Err(_), _) => return Err(denied()),
        };
        if !url.username().is_empty() || url.password().is_some() {
            return Err(denied());
        }
        let granted = match &self.target {
            Target::Loopback(port) => {
                url.scheme() == "http"
                    && is_loopback(&url)
                    && url.port_or_known_default() == Some(*port)
            }
            Target::Entered { origin, .. } => url.origin() == *origin,
        };
        if !granted {
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
        // A marker is filled only once its request is known to go where the person confirmed.
        let target = self.target(url)?;
        if markers::named(url) || markers::named_in(&body) {
            return Err(markers::not_allowed());
        }
        let url = target;
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
            if !markers::named(value) {
                map.append(
                    name,
                    header::HeaderValue::from_str(value).map_err(|_| bad())?,
                );
                continue;
            }
            if markers::NO_SECRET_HEADERS.contains(&name.as_str()) {
                return Err(markers::not_allowed());
            }
            let filled: Zeroizing<String> = markers::fill(value, &self.secrets)?;
            let mut value = header::HeaderValue::from_str(&filled).map_err(|_| bad())?;
            value.set_sensitive(true);
            map.append(name, value);
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

/// Whether `url` names this machine's loopback: `localhost`, `127.0.0.1` or `[::1]`.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address == Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(address)) => address == Ipv6Addr::LOCALHOST,
        None => false,
    }
}

/// Whether an inventory source may be given `address`: HTTPS with a host, or plain HTTP to the
/// loopback; no user, no password.
pub(crate) fn entered_address_allowed(address: &Url) -> bool {
    let scheme = match address.scheme() {
        "https" => address.host().is_some(),
        "http" => is_loopback(address),
        _ => false,
    };
    scheme && address.username().is_empty() && address.password().is_none()
}

/// The port `address` reaches on this machine's loopback, if it names the loopback: what is held
/// against the ports no plugin reaches.
pub(crate) fn loopback_port(address: &Url) -> Option<u16> {
    is_loopback(address)
        .then(|| address.port_or_known_default())
        .flatten()
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

/// A store without the grant has no `http` linked at all, unless its target is one no plugin
/// reaches (a refused port, no address entered): then this denies every request (and if the
/// linker and the store ever disagree).
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
        let over_budget = || {
            to_wit_failure(failure::coded(
                FailureKind::Other,
                codes::PLUGIN_HTTP_TIME_BUDGET,
            ))
        };
        if !self.http_time_left() {
            return Err(over_budget());
        }
        // The service is waiting, not the plugin thinking (rDownloader's RA-HOST-05) - within
        // the request's time limit and the call's budget (RC-080-69).
        let started = Instant::now();
        let answer = grant.request(&method, &url, &headers, body).await;
        if !self.credit_http_time(started.elapsed(), grant.timeout) {
            return Err(over_budget());
        }
        answer.map_err(to_wit_failure)
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
