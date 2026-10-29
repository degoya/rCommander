//! `files.stat`, the FTP test `connections.test` calls, and how a written location becomes a [`VfsPath`]: a
//! path of the panel's backend, `ftp://<connection name>/<path>` on an FTP connection or
//! `sftp://<connection name>/<path>` on an SSH one (`crate::remote`).

use rc_vfs::{BackendId, VfsPath};

use super::panel::resolve_path;
use crate::{
    ConnectionId, Engine, EngineError, Outcome, PanelId,
    remote::{sftp::strip_sftp_scheme, split_location, strip_ftp_scheme},
};

impl Engine {
    /// `target` as a location: `ftp://<name>/<path>` (or `ftps://`) on that FTP connection,
    /// `sftp://<name>/<path>` on that SSH connection ([`Engine::locate_sftp`]), anything else
    /// resolved against `current` ([`resolve_path`]). `ftp://<name>` alone is the connection's
    /// start directory, or where its login starts ([`Engine::ftp_start`]).
    ///
    /// # Errors
    ///
    /// No FTP connection has the name (`ftp_connection_not_found`), several have it
    /// (`ftp_connection_ambiguous`), or the database fails.
    pub(crate) async fn locate(
        &self,
        current: &VfsPath,
        target: &str,
    ) -> Result<VfsPath, EngineError> {
        if let Some(rest) = strip_sftp_scheme(target) {
            return self.locate_sftp(rest).await;
        }
        let Some(rest) = strip_ftp_scheme(target) else {
            return Ok(resolve_path(current, target));
        };
        let (name, path) = split_location(rest);
        let mut found = self.db()?.connections_named(name).await?;
        found.retain(|connection| connection.kind.is_ftp());
        let connection = match found.len() {
            1 => found.remove(0),
            0 => {
                return Err(EngineError::FtpConnectionNotFound {
                    connection: name.to_owned(),
                });
            }
            count => {
                return Err(EngineError::FtpConnectionAmbiguous {
                    connection: name.to_owned(),
                    count,
                });
            }
        };
        match path {
            Some(path) => Ok(VfsPath {
                backend: BackendId::Ftp {
                    connection_id: connection.id.0,
                },
                path,
            }),
            None => {
                self.ftp_start(connection.id, self.announces_locked_vault())
                    .await
            }
        }
    }

    /// `target` resolved against `panel`'s directory, or the location it writes out; inside
    /// [`Engine::pin_mcp_panels`] against the folder pinned (RC-080-42).
    pub(crate) async fn locate_from(
        &self,
        panel: Option<PanelId>,
        target: &str,
    ) -> Result<VfsPath, EngineError> {
        let id = self.resolve(panel);
        let current = self.mcp_folder(id);
        self.locate(&current, target).await
    }

    /// The FTP part of `connections.test` (RC-060-01, `Engine::test_session` calls it for a
    /// kind that is not SSH): connects and logs in once with what is stored, then disconnects.
    /// A kept session of the connection is neither used nor changed. `announce` as in
    /// `test_session`: false for a call over MCP, which then opens no unlock dialog.
    ///
    /// # Errors
    ///
    /// Not an FTP connection (`ftp_connection_not_found`), or what `Engine::open_vfs` fails
    /// with for one: `vault_locked`, `password_not_stored`, the FTP codes.
    pub async fn test_ftp_connection(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<Outcome, EngineError> {
        let connection = self.db()?.connection(id).await?;
        if !connection.kind.is_ftp() {
            return Err(crate::remote::ftp::not_ftp(&connection));
        }
        drop(self.connect_ftp(id, announce).await?);
        Ok(Outcome::ConnectionTested {
            connection: id,
            protocol: connection.kind,
        })
    }

    /// `files.stat`: one entry, by its location.
    pub(crate) async fn stat_entry(
        &self,
        panel: Option<PanelId>,
        target: &str,
    ) -> Result<Outcome, EngineError> {
        let path = self.locate_from(panel, target).await?;
        let entry = self
            .open_vfs(&path, self.announces_locked_vault())
            .await?
            .stat(&path.path)
            .await?;
        Ok(Outcome::Stat { path, entry })
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
