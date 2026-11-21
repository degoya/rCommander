//! `drives.connections` (RC-110-38): the connections with a session open now, which the drive
//! bar shows as buttons of their own - a click returns to the folder last shown there, its
//! context menu disconnects it (`connections.disconnect`).
//!
//! Open is an SSH session that is up (the pool, `sessions.rs`; its files open over SFTP on it)
//! or a kept FTP or FTPS session (`remote/ftp.rs`). Both announce their start and end as
//! [`crate::EngineEvent::Session`], so the window reads the list again on the event, not on
//! its next poll. A quick connection (Ctrl+N) is listed for the window only: MCP never sees one.

use std::collections::BTreeSet;

use crate::{ConnectionId, ConnectionKind, Engine, OpenConnection, Outcome, sessions::SshState};

impl Engine {
    /// `drives.connections`: every connection with a session open now, by id, each with the
    /// folder a panel showed last on it.
    pub(crate) async fn list_open_connections(&self) -> Outcome {
        let mcp = crate::read_guard::from_mcp();
        let mut connections = Vec::new();
        for id in self.open_connection_ids() {
            if let Some(quick) = self.quick_connection(id) {
                if !mcp {
                    let name = quick.name.clone();
                    connections.push(self.open_connection(id, name, ConnectionKind::Ssh));
                }
                continue;
            }
            let Ok(db) = self.db() else {
                continue;
            };
            match db.connection(id).await {
                Ok(connection) => {
                    connections.push(self.open_connection(id, connection.name, connection.kind));
                }
                // Deleted while its session was closing: no button for it.
                Err(error) => {
                    tracing::debug!(connection = %id, %error, "an open session without its connection");
                }
            }
        }
        Outcome::OpenConnections { connections }
    }

    /// The entry of the open connection `id`.
    fn open_connection(
        &self,
        id: ConnectionId,
        name: String,
        kind: ConnectionKind,
    ) -> OpenConnection {
        let drive = format!("server:{id}");
        let last_dir = self.state().last_shown.get(&drive).cloned();
        OpenConnection {
            drive,
            connection_id: id,
            name,
            kind,
            last_dir,
        }
    }

    /// The connections with an SSH session up or an FTP session kept, in id order.
    fn open_connection_ids(&self) -> BTreeSet<ConnectionId> {
        let mut ids: BTreeSet<ConnectionId> = self
            .inner
            .sessions
            .statuses()
            .into_iter()
            .filter(|status| status.state == SshState::Connected)
            .map(|status| status.connection_id)
            .collect();
        ids.extend(self.inner.remotes.ftp().keys().map(|id| ConnectionId(*id)));
        ids
    }

    /// Whether the connection `id` has a session open now: an SSH session up or an FTP session
    /// kept.
    pub(crate) fn connection_open(&self, id: ConnectionId) -> bool {
        let ssh = self
            .inner
            .sessions
            .status(id)
            .is_some_and(|status| status.state == SshState::Connected);
        ssh || self.inner.remotes.ftp().contains_key(&id.0)
    }

    /// The folder a panel showed last on the drive `drive`, while the connection behind it has
    /// a session open: where a click on its button returns (RC-110-38). `None` for any other
    /// drive, and for a connection whose session is gone - it reconnects as a saved server does.
    pub(crate) fn session_dir(&self, drive: &str) -> Option<rc_vfs::VfsPath> {
        let id = drive.strip_prefix("server:")?.parse::<i64>().ok()?;
        if !self.connection_open(ConnectionId(id)) {
            return None;
        }
        self.state().last_shown.get(drive).cloned()
    }
}

#[cfg(test)]
#[path = "open_connections_tests.rs"]
mod tests;
