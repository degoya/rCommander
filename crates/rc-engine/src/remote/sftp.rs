//! An SSH connection of the tree as an [`SftpVfs`] (RC-060-02): two channels of the
//! connection's pooled session, so SFTP shares the login with the terminal and commands.
//!
//! A location is written `sftp://<connection name>/<path>`; `sftp://<name>` alone is the
//! connection's start directory, or the home directory the server names. The backend is kept
//! and registered with the operations queue, so the file operations reach the server like any
//! other backend; one whose session is gone ([`SftpVfs::is_closed`]: a call found out, or the
//! session ended) is replaced on the next use, which opens the session again. One unused for the
//! session's idle time is dropped, so the session can close ([`kept`]). A locked vault is
//! announced only for the window, never for an MCP call (`announce`). Whether the server folds
//! case is probed when the backend opens, unless `sftp.case_sensitivity` says (RC-080-60).

mod kept;

use std::{path::Path, sync::Arc};

use rc_ops::Backend;
use rc_remote::{
    SftpVfs, SshError,
    sftp::{SftpCase, SftpLimits},
};
use rc_vfs::{BackendId, Vfs, VfsPath};

pub(crate) use self::kept::KeptSftp;
use self::kept::{Leases, Use};
use crate::{
    ConnectionId, ConnectionKind, Engine, EngineError,
    remote::{fold, location, strip_scheme},
    sessions::{ChannelLease, SessionChannel},
    settings::{SFTP_CASE_KEY, stored_setting},
};

/// The scheme of an SFTP location.
pub const SFTP_SCHEME: &str = "sftp://";

/// `sftp://<name>/<path>`, how an SFTP location is shown and written.
#[must_use]
pub fn sftp_location(name: &str, path: &Path) -> String {
    location(SFTP_SCHEME, name, path)
}

/// `target` without its `sftp://`, ignoring ASCII case; `None` when it is no SFTP location.
pub(crate) fn strip_sftp_scheme(target: &str) -> Option<&str> {
    strip_scheme(target, SFTP_SCHEME)
}

/// The case rule of the setting `sftp.case_sensitivity`: probed (`auto`, and anything else), or
/// as it says.
fn case_rule(value: Option<&str>) -> SftpCase {
    match value {
        Some("sensitive") => SftpCase::Sensitive,
        Some("insensitive") => SftpCase::Insensitive,
        _ => SftpCase::Detect,
    }
}

/// The backend of connection `id`.
pub(crate) fn backend(id: ConnectionId) -> BackendId {
    BackendId::Sftp {
        connection_id: id.0,
    }
}

impl Engine {
    /// The SFTP backend of connection `id`: the kept one while its session lives, else a new
    /// one on two fresh channels, kept and registered with the operations queue.
    ///
    /// # Errors
    ///
    /// What keeps the session from coming about (the codes of `connections.connect`:
    /// `vault_locked`, `host_key_unknown`, `ssh_auth_failed`, ...), or the server's refusal of
    /// SFTP (`sftp_unavailable`, `sftp_timeout`, ...).
    pub(crate) async fn open_sftp(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<Arc<SftpVfs>, EngineError> {
        // A quick connection's open backend is the person's alone: never an MCP call's.
        self.quick_for_call(id)?;
        if let Some(open) = self.kept_sftp(id) {
            return Ok(open);
        }
        let names = self.sftp_channel(id, announce).await?;
        let files = self.sftp_channel(id, announce).await?;
        tracing::debug!(connection = %id, "starting SFTP");
        // The backend closes with the pool's session: a loss announced before any call (the
        // keepalive, a server that ends the connection) is not answered once more.
        let gone = names.lease.session().closed_signal();
        let idle = names.lease.idle_close();
        let used = Use::new();
        let leases = Leases::new(vec![names.lease, files.lease], Arc::clone(&used));
        let case = match self.db() {
            Ok(db) => stored_setting(db, SFTP_CASE_KEY).await,
            Err(_) => None,
        };
        let case = case_rule(case.as_ref().and_then(serde_json::Value::as_str));
        let opened = SftpVfs::connect_as(names.channel, files.channel, SftpLimits::default(), case)
            .await?
            .ending_with(gone)
            .holding(Box::new(leases));
        let opened = Arc::new(opened);
        // Another call may have opened one meanwhile; a live one stays, this one closes.
        let (kept, ours) = {
            let mut open = self.inner.remotes.sftp();
            match open.get(&id.0) {
                Some(other) if !other.vfs.is_closed() => {
                    other.used.touch();
                    (Arc::clone(&other.vfs), false)
                }
                _ => {
                    let entry = KeptSftp {
                        vfs: Arc::clone(&opened),
                        used: Arc::clone(&used),
                    };
                    open.insert(id.0, entry);
                    (opened, true)
                }
            }
        };
        let as_vfs: Arc<dyn Vfs> = Arc::<SftpVfs>::clone(&kept);
        self.inner
            .remotes
            .open()
            .insert(backend(id), Arc::clone(&as_vfs));
        self.inner.ops.register(backend(id), Backend::new(as_vfs));
        if let Some(idle) = idle.filter(|_| ours) {
            tokio::spawn(kept::release_when_unused(
                Arc::downgrade(&self.inner),
                id,
                Arc::downgrade(&kept),
                used,
                idle,
            ));
        }
        Ok(kept)
    }

    /// The kept backend of connection `id` while its session lives, its use noted.
    fn kept_sftp(&self, id: ConnectionId) -> Option<Arc<SftpVfs>> {
        let open = self.inner.remotes.sftp();
        let kept = open.get(&id.0).filter(|open| !open.vfs.is_closed())?;
        kept.used.touch();
        Some(Arc::clone(&kept.vfs))
    }

    /// Where a panel starts on connection `id`: its start directory, else the server's home (a
    /// quick connection has none).
    ///
    /// # Errors
    ///
    /// As [`Engine::open_sftp`].
    pub(crate) async fn sftp_start(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<VfsPath, EngineError> {
        let start = match self.quick_connection(id) {
            Some(_) => None,
            None => self.db()?.connection(id).await?.start_directory,
        };
        let path = match start.as_deref() {
            Some(start) if start.starts_with('/') => fold(start),
            _ => self.open_sftp(id, announce).await?.home().to_path_buf(),
        };
        Ok(VfsPath {
            backend: backend(id),
            path,
        })
    }

    /// `sftp://<name>/<path>` (the part after the scheme) as a location; `<name>` alone is the
    /// connection's start directory. A name no saved connection has may be a quick connection's
    /// (`user@host`), as the F5 dialog writes one.
    ///
    /// # Errors
    ///
    /// No SSH or quick connection has the name (`sftp_connection_not_found`), several have it
    /// (`sftp_connection_ambiguous`), or the start directory cannot be had.
    pub(crate) async fn locate_sftp(&self, rest: &str) -> Result<VfsPath, EngineError> {
        let (name, path) = super::split_location(rest);
        let mut found = self.db()?.connections_named(name).await?;
        found.retain(|connection| connection.kind == ConnectionKind::Ssh);
        let id = match found.len() {
            1 => found.remove(0).id,
            0 => self
                .quick_named(name)
                .ok_or_else(|| EngineError::SftpConnectionNotFound {
                    connection: name.to_owned(),
                })?,
            count => {
                return Err(EngineError::SftpConnectionAmbiguous {
                    connection: name.to_owned(),
                    count,
                });
            }
        };
        match path {
            Some(path) => Ok(VfsPath {
                backend: backend(id),
                path,
            }),
            None => self.sftp_start(id, self.announces_locked_vault()).await,
        }
    }

    /// A channel on the session of `id` for SFTP; as `Engine::ssh_channel`, with `announce`
    /// passed on, so an MCP call opens no unlock dialog.
    async fn sftp_channel(
        &self,
        id: ConnectionId,
        announce: bool,
    ) -> Result<SessionChannel, EngineError> {
        let live = self.session(id, announce).await?;
        let lease = ChannelLease::new(Arc::clone(&live));
        match live.session.open_channel().await {
            Ok(channel) => Ok(SessionChannel { channel, lease }),
            // Lost between the look and the channel: once more, on a new session.
            Err(SshError::ConnectionLost) => {
                drop(lease);
                let live = self.session(id, announce).await?;
                let lease = ChannelLease::new(Arc::clone(&live));
                let channel = live.session.open_channel().await?;
                Ok(SessionChannel { channel, lease })
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Drops the kept SFTP backend of `id` and takes it from the operations queue.
    pub(crate) fn forget_sftp(&self, id: ConnectionId) {
        self.inner.remotes.sftp().remove(&id.0);
        self.inner.remotes.open().remove(&backend(id));
        self.inner.ops.unregister(&backend(id));
    }

    /// Drops every kept SFTP backend.
    pub(crate) fn forget_all_sftp(&self) {
        let ids: Vec<i64> = self
            .inner
            .remotes
            .sftp()
            .drain()
            .map(|(id, _)| id)
            .collect();
        for id in ids {
            self.inner.ops.unregister(&backend(ConnectionId(id)));
        }
    }
}

#[cfg(test)]
#[path = "sftp_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "sftp_pair_tests.rs"]
mod pair_tests;

#[cfg(test)]
#[path = "sftp_paths_tests.rs"]
mod paths_tests;

#[cfg(test)]
#[path = "sftp_case_tests.rs"]
mod case_tests;
