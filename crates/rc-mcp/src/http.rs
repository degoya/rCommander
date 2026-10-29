//! The Streamable HTTP transport: `http://127.0.0.1:<port>/mcp`, for the running app.
//!
//! Bound to the loopback address only. Every request passes [`guard`] first: the `Host` must be
//! a loopback name and an `Origin`, when a browser sends one, a loopback origin - a web page
//! that rebinds its own name to 127.0.0.1 is refused by both (DNS rebinding) - and the bearer
//! token must match. rmcp's own loopback `Host` check stays on behind it. `/health` passes the
//! same `Host` and `Origin` checks but wants no token: it answers a nonce with the token's proof
//! ([`McpToken::proof`]), so the probe of `rcommander mcp` never sends the token (RC-080-12).
//!
//! The graceful shutdown waits [`SHUTDOWN_GRACE`] for the requests still open, then gives them
//! up (RC-080-58): a request that hangs never keeps the app's process alive.

use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::{
    McpContext, McpToken, RcMcpServer,
    instance::{self, Instance},
};

/// How long [`HttpHandle::shutdown`] waits for open requests; the window's exit gives the
/// server 2 s in all, so the instance file goes inside them.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(1500);

/// How to serve.
#[derive(Debug, Clone)]
pub struct McpHttpConfig {
    /// The port on 127.0.0.1; 0 picks a free one.
    pub port: u16,
    /// The bearer token every request must carry.
    pub token: McpToken,
    /// Where to write the instance file; `None` writes none.
    pub instance_file: Option<PathBuf>,
}

/// A running HTTP server. Stops on [`HttpHandle::shutdown`] or when dropped; either way the
/// instance file goes.
pub struct HttpHandle {
    addr: SocketAddr,
    stop: CancellationToken,
    task: Option<JoinHandle<std::io::Result<()>>>,
    instance_file: Option<PathBuf>,
}

impl HttpHandle {
    /// The address bound.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The MCP endpoint.
    #[must_use]
    pub fn mcp_url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// Stops accepting, ends the sessions, waits [`SHUTDOWN_GRACE`] for the server and removes
    /// the instance file.
    ///
    /// # Errors
    ///
    /// The server failed while running.
    pub async fn shutdown(self) -> anyhow::Result<()> {
        self.shutdown_within(SHUTDOWN_GRACE).await
    }

    /// [`HttpHandle::shutdown`] with its grace: after it the server's task is aborted - the
    /// listener closes with it -, and a request still open ends with the runtime.
    async fn shutdown_within(mut self, grace: Duration) -> anyhow::Result<()> {
        self.stop.cancel();
        if let Some(mut task) = self.task.take() {
            match tokio::time::timeout(grace, &mut task).await {
                Ok(joined) => joined??,
                Err(_) => {
                    tracing::debug!(
                        ?grace,
                        "MCP requests still open after the grace are given up"
                    );
                    task.abort();
                }
            }
        }
        self.remove_instance_file();
        Ok(())
    }

    fn remove_instance_file(&mut self) {
        if let Some(path) = self.instance_file.take() {
            instance::remove_own(&path, self.addr.port());
        }
    }
}

impl Drop for HttpHandle {
    fn drop(&mut self) {
        self.stop.cancel();
        self.remove_instance_file();
    }
}

/// Starts the server on `127.0.0.1:<port>` and writes the instance file.
///
/// # Errors
///
/// The port cannot be bound or the instance file cannot be written.
pub async fn serve_http(context: McpContext, config: McpHttpConfig) -> anyhow::Result<HttpHandle> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, config.port)).await?;
    let addr = listener.local_addr()?;
    let stop = CancellationToken::new();
    let service = StreamableHttpService::new(
        move || Ok(RcMcpServer::new(context.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_cancellation_token(stop.child_token()),
    );
    let token = Arc::new(config.token.clone());
    let prover = Arc::clone(&token);
    let port = addr.port();
    // No secret source's `http` reaches this port, whatever its manifest grants (RC-100-34).
    rc_engine::secret_sources::note_mcp_port(port);
    // The token guard wraps the routes before it only: `/health` checks host and origin itself.
    let app = Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(token, guard))
        .route(
            "/health",
            get(move |headers: HeaderMap, uri: Uri| {
                std::future::ready(health(&prover, port, &headers, &uri))
            }),
        );
    let mut handle = HttpHandle {
        addr,
        stop: stop.clone(),
        task: None,
        instance_file: None,
    };
    if let Some(path) = config.instance_file {
        instance::write(
            &path,
            &Instance {
                port: addr.port(),
                token: config.token.expose().to_owned(),
                pid: std::process::id(),
            },
        )?;
        handle.instance_file = Some(path);
    }
    handle.task = Some(spawn_server(listener, app, stop));
    Ok(handle)
}

/// Serves `app` on `listener` until `stop` is cancelled and every open request has ended.
fn spawn_server(
    listener: TcpListener,
    app: Router,
    stop: CancellationToken,
) -> JoinHandle<std::io::Result<()>> {
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
    })
}

/// What `rcommander mcp` probes before it forwards: a loopback request with a nonce is answered
/// with the token's proof for this port and that nonce - the token is neither wanted nor shown.
fn health(token: &McpToken, port: u16, headers: &HeaderMap, uri: &Uri) -> Response {
    if let Err(refusal) = check_loopback(headers) {
        return refusal.into_response();
    }
    let Some(nonce) = uri.query().and_then(nonce_of) else {
        return Refusal {
            status: StatusCode::BAD_REQUEST,
            code: "nonce_required",
            message: "the probe sends a nonce of 32 to 128 hex digits",
        }
        .into_response();
    };
    Json(serde_json::json!({
        "app": "rcommander",
        "proof": token.proof(port, nonce).to_hex().as_str(),
    }))
    .into_response()
}

/// The `nonce` of a query string: 32 to 128 hex digits, or `None`.
fn nonce_of(query: &str) -> Option<&str> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("nonce="))
        .filter(|nonce| {
            (32..=128).contains(&nonce.len()) && nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

/// Host, Origin and token, before anything else runs.
async fn guard(State(token): State<Arc<McpToken>>, request: Request, next: Next) -> Response {
    match check(&token, request.headers()) {
        Ok(()) => next.run(request).await,
        Err(refusal) => refusal.into_response(),
    }
}

/// Why a request is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) status: StatusCode,
    pub(crate) code: &'static str,
    message: &'static str,
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(serde_json::json!({ "error": self.message, "code": self.code })),
        )
            .into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}

/// The checks every request to `/mcp` passes: [`check_loopback`] and the bearer token.
pub(crate) fn check(token: &McpToken, headers: &HeaderMap) -> Result<(), Refusal> {
    check_loopback(headers)?;
    let offered = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split_once(' ')
                .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        })
        .map(|(_, offered)| offered.trim());
    if !offered.is_some_and(|offered| token.matches(offered)) {
        return Err(Refusal {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "a valid bearer token is required",
        });
    }
    Ok(())
}

/// The checks every request passes, `/health` too: a loopback `Host`, and a loopback `Origin`
/// when there is one.
fn check_loopback(headers: &HeaderMap) -> Result<(), Refusal> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !is_loopback_authority(host) {
        return Err(Refusal {
            status: StatusCode::FORBIDDEN,
            code: "host_rejected",
            message: "only 127.0.0.1 and localhost are served",
        });
    }
    if let Some(origin) = headers.get(header::ORIGIN)
        && !origin.to_str().is_ok_and(is_loopback_origin)
    {
        return Err(Refusal {
            status: StatusCode::FORBIDDEN,
            code: "origin_rejected",
            message: "requests from web pages are refused",
        });
    }
    Ok(())
}

/// Whether a host name (without port) is the loopback.
fn is_loopback_host(host: &str) -> bool {
    matches!(
        host.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "localhost" | "[::1]"
    )
}

/// `Host`: a loopback name with an optional port.
fn is_loopback_authority(authority: &str) -> bool {
    let host = match authority.rsplit_once(':') {
        Some((host, port))
            if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            host
        }
        _ => authority,
    };
    is_loopback_host(host)
}

/// `Origin`: `http(s)://` and a loopback host; `null` and everything else is refused.
fn is_loopback_origin(origin: &str) -> bool {
    url::Url::parse(origin).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some_and(is_loopback_host)
            && url.username().is_empty()
    })
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
