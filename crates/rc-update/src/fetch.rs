//! Reaching the release host: https at every hop, bounded, and replaceable in tests (RC-100-05).
//!
//! The engine holds a [`Fetcher`] rather than a client, so a test serves a manifest and an
//! artifact from memory ([`MemoryFetcher`]) without a TLS server, and the production
//! [`HttpFetcher`] refuses to leave TLS at any redirect. A request carries the user agent
//! `rCommander/<version>` and nothing else of this installation: no cookie store, no query, no
//! installation id. TLS checks against the system's roots (`rustls-platform-verifier`), so a
//! company proxy with its own root works; the signature protects the content either way.
//!
//! Copied and adapted from rDownloader's `crates/rd-update/src/fetch.rs` at commit 26c8592a6: the
//! proxy setting, typed refusals for a proxy login and an oversized body, chunks without `bytes`.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
/// The address type of [`Fetcher`], for an implementation outside this crate.
pub use url::Url;

/// How long fetching one small document (a manifest, the release list) may take.
pub const FETCH_TIMEOUT_SECONDS: u64 = 60;
/// How long connecting may take.
pub const CONNECT_TIMEOUT_SECONDS: u64 = 30;
/// How long a download may go without receiving a byte.
pub const READ_TIMEOUT_SECONDS: u64 = 60;
/// Most redirects one request follows; a release asset takes two.
pub const MAX_REDIRECTS: usize = 5;

/// A body larger than the caller's limit: the check reports it as `update.too_large`.
#[derive(Debug, thiserror::Error)]
#[error("the answer is larger than {limit} bytes")]
pub struct BodyTooLarge {
    pub limit: u64,
}

/// The proxy answered 407: a login this check never gives (`update.proxy_auth_required`).
#[derive(Debug, thiserror::Error)]
#[error("the proxy asks for a login")]
pub struct ProxyAuthRequired;

/// The setting `update.proxy`: empty for the system's proxy (the Windows Internet options,
/// `HTTPS_PROXY`/`NO_PROXY`), `direct` for none, or `http(s)://host:port` without a login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyChoice {
    System,
    Direct,
    Address(String),
}

impl ProxyChoice {
    /// Reads the setting's text.
    ///
    /// # Errors
    ///
    /// Why the text is no proxy this check uses.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Self::System);
        }
        if text == "direct" {
            return Ok(Self::Direct);
        }
        let url = url::Url::parse(text).map_err(|error| format!("not an address: {error}"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("only http:// and https:// proxies".to_owned());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("a proxy address carries no login".to_owned());
        }
        if url.host_str().is_none_or(str::is_empty)
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("expected http(s)://host:port".to_owned());
        }
        Ok(Self::Address(text.to_owned()))
    }
}

/// `rCommander/<version>`, all a request says about this installation.
#[must_use]
pub fn user_agent() -> String {
    format!("rCommander/{}", env!("CARGO_PKG_VERSION"))
}

/// The bytes of an opened download, chunk by chunk.
#[async_trait::async_trait]
pub trait Body: Send {
    /// The next chunk; `None` at the end.
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>>;
}

/// An opened download: its declared length, when the server named one, and its bytes.
pub struct Download {
    pub length: Option<u64>,
    pub body: Box<dyn Body>,
}

/// Reads what the update check and the download need.
#[async_trait::async_trait]
pub trait Fetcher: Send + Sync {
    /// Fetches `url`, refusing a body longer than `limit` bytes ([`BodyTooLarge`]). `Ok(None)`
    /// is a `404`: nothing published there.
    async fn fetch(&self, url: &url::Url, limit: u64) -> Result<Option<Vec<u8>>>;
    /// Opens `url` for streaming.
    async fn open(&self, url: &url::Url) -> Result<Download>;
}

/// The production fetcher: `reqwest`, https at every hop.
pub struct HttpFetcher {
    client: reqwest::Client,
}

impl HttpFetcher {
    /// A client through `proxy`.
    #[must_use]
    pub fn new(proxy: &ProxyChoice) -> Self {
        let redirects = reqwest::redirect::Policy::custom(|attempt| {
            match refuse_redirect(attempt.url(), attempt.previous().len()) {
                Some(reason) => attempt.error(reason),
                None => attempt.follow(),
            }
        });
        let mut builder = reqwest::Client::builder()
            // GitHub's API refuses a request without one.
            .user_agent(user_agent())
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECONDS))
            .read_timeout(Duration::from_secs(READ_TIMEOUT_SECONDS))
            .redirect(redirects)
            .https_only(true);
        builder = match proxy {
            ProxyChoice::System => builder,
            ProxyChoice::Direct => builder.no_proxy(),
            ProxyChoice::Address(address) => match reqwest::Proxy::all(address.as_str()) {
                Ok(proxy) => builder.proxy(proxy),
                Err(error) => {
                    tracing::warn!(%error, "the update proxy is not usable; the system's applies");
                    builder
                }
            },
        };
        Self {
            client: builder.build().unwrap_or_default(),
        }
    }

    async fn get(&self, url: &url::Url) -> Result<Option<reqwest::Response>> {
        ensure!(url.scheme() == "https", "{url} is not https");
        let response = self
            .client
            .get(url.clone())
            .header(
                reqwest::header::ACCEPT,
                "application/json, application/octet-stream",
            )
            .send()
            .await
            .with_context(|| format!("fetch {url}"))?;
        match response.status() {
            reqwest::StatusCode::NOT_FOUND => Ok(None),
            reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED => Err(ProxyAuthRequired.into()),
            status if status.is_success() => Ok(Some(response)),
            status => bail!("{url} answered {status}"),
        }
    }
}

/// Why a redirect to `target`, after `hops` earlier ones, is not followed; `None` follows it.
pub(crate) fn refuse_redirect(target: &url::Url, hops: usize) -> Option<&'static str> {
    if target.scheme() != "https" {
        Some("an update address redirected to a non-https address")
    } else if hops >= MAX_REDIRECTS {
        Some("an update address redirected too often")
    } else {
        None
    }
}

struct HttpBody(reqwest::Response);

#[async_trait::async_trait]
impl Body for HttpBody {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self
            .0
            .chunk()
            .await
            .context("read the download")?
            .map(|chunk| chunk.to_vec()))
    }
}

#[async_trait::async_trait]
impl Fetcher for HttpFetcher {
    async fn fetch(&self, url: &url::Url, limit: u64) -> Result<Option<Vec<u8>>> {
        let read = async {
            let Some(mut response) = self.get(url).await? else {
                return Ok(None);
            };
            // The declared length first, so an oversized body is refused before it is read;
            // the running count catches a server that declares less than it sends.
            if response
                .content_length()
                .is_some_and(|length| length > limit)
            {
                return Err(BodyTooLarge { limit }.into());
            }
            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .with_context(|| format!("read {url}"))?
            {
                body.extend_from_slice(&chunk);
                if u64::try_from(body.len()).unwrap_or(u64::MAX) > limit {
                    return Err(BodyTooLarge { limit }.into());
                }
            }
            Ok(Some(body))
        };
        tokio::time::timeout(Duration::from_secs(FETCH_TIMEOUT_SECONDS), read)
            .await
            .map_err(|_| {
                anyhow::anyhow!("no answer from {url} within {FETCH_TIMEOUT_SECONDS} seconds")
            })?
    }

    async fn open(&self, url: &url::Url) -> Result<Download> {
        let response = self
            .get(url)
            .await?
            .with_context(|| format!("{url} answered 404 Not Found"))?;
        Ok(Download {
            length: response.content_length(),
            body: Box::new(HttpBody(response)),
        })
    }
}

/// A fetcher that serves fixed bodies from memory and records what was asked of it; an address
/// it does not hold answers `404`. For the tests here and in `rc-engine`.
#[derive(Clone, Default)]
pub struct MemoryFetcher {
    bodies: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl MemoryFetcher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Serves `body` at `url` from now on.
    pub fn serve(&self, url: &str, body: impl Into<Vec<u8>>) {
        if let Ok(mut bodies) = self.bodies.lock() {
            bodies.insert(url.to_owned(), body.into());
        }
    }

    /// Every address asked for so far, in order.
    #[must_use]
    pub fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .map(|requests| requests.clone())
            .unwrap_or_default()
    }

    fn body(&self, url: &url::Url) -> Option<Vec<u8>> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(url.to_string());
        }
        self.bodies.lock().ok()?.get(url.as_str()).cloned()
    }
}

struct MemoryBody(std::vec::IntoIter<Vec<u8>>);

#[async_trait::async_trait]
impl Body for MemoryBody {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.0.next())
    }
}

#[async_trait::async_trait]
impl Fetcher for MemoryFetcher {
    async fn fetch(&self, url: &url::Url, limit: u64) -> Result<Option<Vec<u8>>> {
        let Some(body) = self.body(url) else {
            return Ok(None);
        };
        if u64::try_from(body.len()).unwrap_or(u64::MAX) > limit {
            return Err(BodyTooLarge { limit }.into());
        }
        Ok(Some(body))
    }

    async fn open(&self, url: &url::Url) -> Result<Download> {
        let body = self
            .body(url)
            .with_context(|| format!("{url} answered 404 Not Found"))?;
        let length = u64::try_from(body.len()).ok();
        // Several chunks, as a network delivers them, so the streaming path is what is tested.
        let chunks: Vec<Vec<u8>> = body.chunks(7).map(<[u8]>::to_vec).collect();
        Ok(Download {
            length,
            body: Box::new(MemoryBody(chunks.into_iter())),
        })
    }
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
