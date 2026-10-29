//! The stdio transport: `rcommander mcp`.
//!
//! When a running rCommander answers (its instance file and [`instance::probe`]), stdio is
//! forwarded to it, so the agent drives the panels the person sees. Otherwise an engine starts
//! headless in the current directory and serves stdio itself - unless the server is switched off
//! in the settings, which ends it with an error that says so. From WSL this is how Claude Code
//! reaches the Windows app: stdio crosses the interop, where WSL2's NAT would hide Windows'
//! localhost.

use std::{future::Future, path::PathBuf, pin::Pin, time::Duration};

use rc_db::Db;
use rc_engine::{Engine, VfsPath};
use rc_vault::Vault;
use rmcp::{
    RoleServer, ServiceExt as _,
    model::{JsonRpcMessage, RequestId, ServerJsonRpcMessage},
    transport::{
        StreamableHttpClientTransport, Transport, async_rw::AsyncRwTransport,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
    time::{self, MissedTickBehavior},
};

use crate::{
    McpContext, RcMcpServer,
    instance::{self, Instance},
};

/// `rcommander mcp`: forward to the running app, or serve headless.
///
/// # Errors
///
/// The database cannot be opened, the start directory cannot be listed, or the transport fails.
pub async fn run_stdio() -> anyhow::Result<()> {
    let (read, write) = rmcp::transport::stdio();
    if let Ok(path) = instance::default_path()
        && let Some(running) = instance::read(&path)
        && instance::probe(&running).await
    {
        return proxy_io(&running, read, write).await;
    }
    let db = Db::open(Db::default_path()?).await?;
    // Warn and error lines into the error log, the console on stderr (stdout is MCP's).
    rc_db::install_diagnostics(&db);
    if !crate::config::enabled(&db).await {
        anyhow::bail!(crate::config::DISABLED_MESSAGE);
    }
    let start = std::env::current_dir().unwrap_or_else(|_| fallback_start());
    let engine = Engine::headless(VfsPath::local(start)).await?;
    engine.attach_db(db.clone());
    // Headless, the vault can only be read and locked: nobody is there to type its password.
    engine.attach_vault(Vault::open(db.clone()).await?);
    serve_io(McpContext::new(engine, db), read, write).await
}

/// Where a headless engine starts when the current directory is unknown.
fn fallback_start() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    }
}

/// Serves one session over a byte stream pair until the client closes it.
///
/// # Errors
///
/// The session cannot start or ends with a transport error.
pub async fn serve_io<R, W>(context: McpContext, read: R, write: W) -> anyhow::Result<()>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let service = RcMcpServer::new(context).serve((read, write)).await?;
    service.waiting().await?;
    Ok(())
}

/// How often the proxy asks whether the running instance is still there.
const ALIVE_EVERY: Duration = Duration::from_secs(3);

/// Probes in a row the instance must miss before the proxy ends; one may be a busy moment.
const ALIVE_MISSES: u32 = 2;

/// One upstream send, in the order the client sent it, with the id to answer a failure to.
type QueuedSend = (
    Option<RequestId>,
    Pin<Box<dyn Future<Output = Result<(), String>> + Send>>,
);

/// Forwards a byte stream pair to a running instance's HTTP endpoint until either side ends.
///
/// rmcp's HTTP client does not notice an instance that went away, so the proxy probes it every
/// [`ALIVE_EVERY`] and ends after [`ALIVE_MISSES`] misses: an `rcommander mcp` left running
/// would reach nothing - a new instance has a new port and token - and would hold the
/// program's file, which then cannot be replaced (owner 2026-10-25).
///
/// Messages pass as JSON-RPC, unchanged; rmcp's HTTP client keeps the MCP session. Sends go
/// upstream one at a time in the client's order on their own task, so answers keep flowing
/// back while a request is still being posted. A request the instance did not take is answered
/// with an error rather than left hanging.
///
/// # Errors
///
/// Writing to the client fails.
pub async fn proxy_io<R, W>(running: &Instance, read: R, write: W) -> anyhow::Result<()>
where
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let mut client = AsyncRwTransport::<RoleServer, R, W>::new_server(read, write);
    let mut upstream = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(running.mcp_url())
            .auth_header(running.token.clone()),
    );
    let (queue, mut queued) = mpsc::unbounded_channel::<QueuedSend>();
    let (failures, mut failed) = mpsc::unbounded_channel::<(RequestId, String)>();
    let sender = tokio::spawn(async move {
        while let Some((id, send)) = queued.recv().await {
            if let Err(error) = send.await
                && let Some(id) = id
            {
                let _ = failures.send((id, error));
            }
        }
    });
    let mut alive = time::interval_at(time::Instant::now() + ALIVE_EVERY, ALIVE_EVERY);
    alive.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut misses = 0;
    let result = loop {
        tokio::select! {
            message = client.receive() => {
                let Some(message) = message else { break Ok(()) };
                let id = match &message {
                    JsonRpcMessage::Request(request) => Some(request.id.clone()),
                    _ => None,
                };
                let send = upstream.send(message);
                let send: Pin<Box<dyn Future<Output = Result<(), String>> + Send>> =
                    Box::pin(async move { send.await.map_err(|error| error.to_string()) });
                if queue.send((id, send)).is_err() {
                    break Ok(());
                }
            }
            message = upstream.receive() => {
                let Some(message) = message else { break Ok(()) };
                if let Err(error) = client.send(message).await {
                    break Err(anyhow::Error::from(error));
                }
            }
            _ = alive.tick() => {
                misses = if instance::probe(running).await { 0 } else { misses + 1 };
                if misses >= ALIVE_MISSES {
                    eprintln!("rcommander mcp: the running rCommander ended; so does this bridge");
                    break Ok(());
                }
            }
            Some((id, error)) = failed.recv() => {
                let answer = ServerJsonRpcMessage::error(
                    rmcp::ErrorData::internal_error(
                        format!("the running rCommander did not take the request: {error}"),
                        None,
                    ),
                    Some(id),
                );
                if let Err(error) = client.send(answer).await {
                    break Err(anyhow::Error::from(error));
                }
            }
        }
    };
    drop(queue);
    let _ = sender.await;
    let _ = upstream.close().await;
    let _ = client.close().await;
    result
}
