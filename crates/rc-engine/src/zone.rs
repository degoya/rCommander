//! The engine's side of the Mark-of-the-Web (RC-110-56, `rc_ops::zone`): the server a file came
//! from as its `HostUrl` - for the queue's copies through the [`rc_ops::Reconnect`] hook - and
//! the zone of the copy Enter unpacks into the temporary folder from an archive, or fetches from
//! a server inside an MCP call, before a program of the system opens it.

use std::{io, path::Path};

use rc_ops::zone::{self, Origin};
use rc_vfs::{BackendId, VfsPath};

use crate::recover::LogPanic as _;
use crate::{ConnectionId, ConnectionKind, Engine};

/// A saved connection as the `HostUrl` of a file copied from it: scheme, host and port, never a
/// user name or anything secret.
pub(crate) fn host_url(kind: ConnectionKind, host: &str, port: u16) -> String {
    let scheme = match kind {
        ConnectionKind::Ssh => "sftp",
        ConnectionKind::Ftp => "ftp",
        ConnectionKind::Ftps | ConnectionKind::FtpsImplicit => "ftps",
    };
    // An IPv6 address goes in brackets, as in any URL.
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    format!("{scheme}://{host}:{port}/")
}

impl Engine {
    /// The server `backend` reaches as a `HostUrl`; `None` for a connection the database does
    /// not hold (a quick connection) and for every other backend.
    pub(crate) async fn server_url(&self, backend: &BackendId) -> Option<String> {
        let id = backend.connection_id().map(ConnectionId)?;
        let connection = self.db().ok()?.connection(id).await.ok()?;
        Some(host_url(connection.kind, &connection.host, connection.port))
    }

    /// Gives `file`, a copy of `source` on this machine, the zone `source` carries. A zone that
    /// cannot be written is logged and opens the file all the same.
    pub(crate) async fn keep_zone(&self, source: &VfsPath, file: &Path) {
        if !zone::SUPPORTED {
            return;
        }
        let origin = zone::origin(&source.backend);
        let host = match &origin {
            Origin::Server(server) => self.server_url(server).await,
            _ => None,
        };
        let (from, to) = (source.path.clone(), file.to_path_buf());
        let kept = tokio::task::spawn_blocking(move || {
            zone::keep(&zone::carry(&origin, host.as_deref()), &from, &to)
        })
        .await
        .log_panic("engine.zone")
        .map_err(io::Error::other)
        .and_then(|kept| kept);
        if let Err(error) = kept {
            tracing::warn!(%error, "a file unpacked to be opened did not keep its zone");
        }
    }
}

#[cfg(test)]
#[path = "zone_tests.rs"]
mod tests;
