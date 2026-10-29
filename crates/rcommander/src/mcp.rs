//! The MCP server inside the running app: Streamable HTTP on 127.0.0.1 over the window's own
//! engine, so an agent drives the panels on screen. `rcommander mcp` (stdio) finds it through the
//! instance file and forwards to it.
//!
//! On unless the setting `mcp.http.enabled` is `false`; the port is `mcp.http.port`, any free one
//! when it is 0 or unset (`rc_mcp::config`). The settings dialog (RC-010-09) edits both and the
//! token, and [`restart`]s the server with them at once. A server that cannot start leaves the
//! app running and says why in the status bar.

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
};

use rc_db::Db;
use rc_engine::Engine;
use rc_mcp::{HttpHandle, McpContext, McpHttpConfig, McpToken, config, serve_http};

/// What the status bar says about the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpStatus {
    /// Switched off in the settings.
    Off,
    /// Serving on this address.
    Serving(SocketAddr),
    /// It could not start, for this reason (never with the token in it).
    Failed(String),
}

/// The running server, shared by the window (which restarts it) and the start (which stops it
/// after the window closed).
pub type McpServer = Arc<Mutex<Option<HttpHandle>>>;

/// Starts the server as the settings in `db` say, writing `instance_file` when given. The handle
/// stops it (and removes the instance file) when shut down or dropped.
pub async fn start(
    engine: Engine,
    db: Db,
    instance_file: Option<PathBuf>,
) -> (Option<HttpHandle>, McpStatus) {
    if !config::enabled(&db).await {
        return (None, McpStatus::Off);
    }
    let port = config::port(&db).await;
    let token = match McpToken::load_or_create(&db).await {
        Ok(token) => token,
        Err(error) => return (None, McpStatus::Failed(error.to_string())),
    };
    let config = McpHttpConfig {
        port,
        token,
        instance_file,
    };
    match serve_http(McpContext::new(engine, db), config).await {
        Ok(handle) => {
            let addr = handle.addr();
            (Some(handle), McpStatus::Serving(addr))
        }
        Err(error) => (None, McpStatus::Failed(format!("{error:#}"))),
    }
}

/// Stops the server in `server`, if one runs, and starts it again as the settings now say: off,
/// on another port, or with a new token - the old token stops working.
pub async fn restart(
    server: McpServer,
    engine: Engine,
    db: Db,
    instance_file: Option<PathBuf>,
) -> McpStatus {
    let running = server.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(running) = running {
        // A server that failed while it ran is replaced all the same.
        let _ = running.shutdown().await;
    }
    let (handle, status) = start(engine, db, instance_file).await;
    *server.lock().unwrap_or_else(PoisonError::into_inner) = handle;
    status
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
