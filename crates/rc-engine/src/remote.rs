//! Remote backends: the file systems of the connection tree's connections, opened on first use
//! and kept for the calls that follow. FTP and FTPS (RC-070-01, [`ftp`]) and SFTP on the SSH
//! connections (RC-060-02, [`sftp`], `sftp://<connection name>/<path>`).
//!
//! A remote location is written `ftp://<connection name>/<path>` (or `ftps://`, the same
//! connection: its encryption is its own, RC-070-03): the name finds the connection
//! (in any folder, ignoring ASCII case), the path is absolute on the server. Inside the engine it
//! is a [`VfsPath`] on [`BackendId::Ftp`], which carries the connection's id, so a renamed
//! connection keeps its open locations (`Engine::locate`, `commands/remote.rs`, reads the
//! written form). [`Engine::open_vfs`] turns such a path into its
//! backend: the kept session, or a new one, logged in with the password from the vault
//! ([`Engine::resolve_secret`]: a locked vault answers `vault_locked`, nothing stored
//! `password_not_stored`). A changed or deleted connection, or a new or cleared secret of it,
//! drops its session; the next call logs in again with what is stored now. A locked vault
//! leaves an open session open, as it leaves a running transfer alone.

pub(crate) mod ftp;
pub(crate) mod quick;
pub(crate) mod sftp;

use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use rc_remote::FtpVfs;
use rc_vfs::{BackendId, Vfs, VfsPath};

use crate::{ConnectionId, ConnectionKind, Engine, EngineError, EngineEvent, PanelId};

/// The scheme of an FTP location; FTPS connections use it too, their encryption is the
/// connection's.
pub const FTP_SCHEME: &str = "ftp://";
/// `ftps://` names an FTP connection too, as the path prompt is typed; the connection decides.
const FTPS_SCHEME: &str = "ftps://";

/// The open remote backends, by backend.
#[derive(Default)]
pub(crate) struct Remotes {
    open: Mutex<HashMap<BackendId, Arc<dyn Vfs>>>,
    /// The SFTP backends among them, by connection id: their home and whether they still live.
    sftp: Mutex<HashMap<i64, sftp::KeptSftp>>,
    /// The FTP backends among them, by connection id: their home (RC-070-03).
    ftp: Mutex<HashMap<i64, FtpVfs>>,
    /// Counts the drops of kept FTP sessions, under the `ftp` lock: a session that started
    /// before one is not kept, its data may be old (RC-080-51).
    ftp_forgets: AtomicU64,
    /// The unsaved quick connections (Ctrl+N), by their negative ids.
    pub(crate) quick: quick::QuickConnections,
}

impl Remotes {
    pub(crate) fn open(&self) -> MutexGuard<'_, HashMap<BackendId, Arc<dyn Vfs>>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn sftp(&self) -> MutexGuard<'_, HashMap<i64, sftp::KeptSftp>> {
        self.sftp.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn ftp(&self) -> MutexGuard<'_, HashMap<i64, FtpVfs>> {
        self.ftp.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many times kept FTP sessions were dropped; read under the `ftp` lock.
    pub(crate) fn ftp_forgets(&self) -> u64 {
        self.ftp_forgets.load(Ordering::SeqCst)
    }

    /// Notes a drop of kept FTP sessions; called under the `ftp` lock.
    pub(crate) fn note_ftp_forget(&self) {
        self.ftp_forgets.fetch_add(1, Ordering::SeqCst);
    }
}

/// `ftp://<name>/<path>`, how an FTP location is shown and written.
#[must_use]
pub fn ftp_location(name: &str, path: &Path) -> String {
    location(FTP_SCHEME, name, path)
}

/// `<scheme><name>/<path>`, how a remote location is shown and written: the path's names
/// joined with `/`, the root as `/`.
pub(crate) fn location(scheme: &str, name: &str, path: &Path) -> String {
    let mut location = format!("{scheme}{name}");
    let mut empty = true;
    for component in path.components() {
        if let Component::Normal(part) = component {
            location.push('/');
            location.push_str(&part.to_string_lossy());
            empty = false;
        }
    }
    if empty {
        location.push('/');
    }
    location
}

/// `rest` of `ftp://<rest>` split into the connection's name and the absolute path after it;
/// `None` for the path when the location names only the connection.
pub(crate) fn split_location(rest: &str) -> (&str, Option<PathBuf>) {
    match rest.split_once('/') {
        Some((name, path)) => (name, Some(fold(path))),
        None => (rest, None),
    }
}

/// `path` as an absolute path: split at `/`, `.` and empty parts dropped, `..` folded (at the
/// root it stays at the root, as everywhere else).
pub(crate) fn fold(path: &str) -> PathBuf {
    let mut folded = PathBuf::from("/");
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                folded.pop();
            }
            name => folded.push(name),
        }
    }
    folded
}

/// `target` without its `ftp://` or `ftps://`, ignoring ASCII case; `None` when it is no FTP
/// location.
pub(crate) fn strip_ftp_scheme(target: &str) -> Option<&str> {
    [FTP_SCHEME, FTPS_SCHEME]
        .into_iter()
        .find_map(|scheme| strip_scheme(target, scheme))
}

/// `target` without its `scheme`, ignoring ASCII case; `None` when it does not start with it.
pub(crate) fn strip_scheme<'a>(target: &'a str, scheme: &str) -> Option<&'a str> {
    let head = target.get(..scheme.len())?;
    head.eq_ignore_ascii_case(scheme)
        .then(|| &target[scheme.len()..])
}

impl Engine {
    /// The backend of `path`, a remote one opened and logged in first when it is not open yet.
    /// A locked vault is announced ([`crate::EngineEvent::SecretNeeded`]) only when `announce`:
    /// true from the window, false for a call over MCP, which gets `vault_locked` and opens no
    /// unlock dialog.
    ///
    /// # Errors
    ///
    /// As [`Engine::vfs`]; for a remote backend also what keeps its session from coming about
    /// (the connection gone, `vault_locked`, `password_not_stored`, an FTP failure's code, an
    /// SSH session's or SFTP's code).
    pub(crate) async fn open_vfs(
        &self,
        path: &VfsPath,
        announce: bool,
    ) -> Result<Arc<dyn Vfs>, EngineError> {
        if let BackendId::Sftp { connection_id } = path.backend {
            let sftp: Arc<dyn Vfs> = self
                .open_sftp(ConnectionId(connection_id), announce)
                .await?;
            return Ok(sftp);
        }
        let BackendId::Ftp { connection_id } = path.backend else {
            return self.vfs(path);
        };
        if let Some(open) = self.inner.remotes.open().get(&path.backend) {
            return Ok(Arc::clone(open));
        }
        let ftp = self.open_ftp(ConnectionId(connection_id), announce).await?;
        Ok(Arc::new(ftp))
    }

    /// Tells the window that a session to the plain-FTP connection `id` came up: its user,
    /// password and data cross the network in clear (RC-080-12). Nothing for FTPS.
    pub(crate) async fn warn_if_plain_ftp(&self, id: ConnectionId) {
        let Ok(db) = self.db() else {
            return;
        };
        if let Ok(connection) = db.connection(id).await
            && connection.kind == ConnectionKind::Ftp
        {
            self.publish(EngineEvent::PlainFtp {
                connection: id,
                name: connection.name,
            });
        }
    }

    /// Drops the kept session of connection `id`, if there is one: after the connection changed
    /// or went, or on `connections.disconnect`, the next call logs in with what is stored now.
    /// Whether an FTP session was kept (it is closed, `QUIT` and all); an SFTP backend goes with
    /// its connection's too.
    pub(crate) async fn forget_remote(&self, id: ConnectionId) -> bool {
        let ftp = self.forget_ftp(id).await;
        self.forget_sftp(id);
        ftp
    }

    /// After `connections.disconnect` (Ctrl+Shift+F), as Total Commander does: each panel that
    /// shows connection `id` leaves the server for the last local folder its tab showed before
    /// (its history), else the other panel's local folder, else the home directory - the first
    /// of them that lists. A locked tab opens it in a new tab, as for any other folder; the
    /// panel's other tabs stay and log in again when shown. Owner, 2026-10-18 (RC-070-03).
    pub(crate) async fn leave_server(&self, id: ConnectionId) {
        for panel in [PanelId::Left, PanelId::Right] {
            let choices = {
                let state = self.state();
                if !shows_connection(&state.panel(panel).path, id) {
                    continue;
                }
                let history = &state.tabs.set(panel).active_tab().history;
                let mut choices: Vec<VfsPath> = history.entries()[..history.pos()]
                    .iter()
                    .rev()
                    .filter(|path| path.backend == BackendId::Local)
                    .cloned()
                    .collect();
                let other = &state.panel(panel.other()).path;
                if other.backend == BackendId::Local {
                    choices.push(other.clone());
                }
                choices
            };
            let home = directories::BaseDirs::new().map(|dirs| VfsPath::local(dirs.home_dir()));
            for path in choices.into_iter().chain(home) {
                if self.show_path(panel, path).await.is_ok() {
                    break;
                }
            }
        }
    }

    /// Whether a command that needs a stored secret announces a locked vault: only for the
    /// window, never in an MCP dispatch (`read_guard::from_mcp`).
    pub(crate) fn announces_locked_vault(&self) -> bool {
        !crate::read_guard::from_mcp()
    }

    /// Drops every kept session: after a folder went with connections inside.
    pub(crate) async fn forget_remotes(&self) {
        self.forget_all_sftp();
        self.inner.remotes.open().clear();
        self.forget_all_ftp().await;
    }
}

/// Whether `path` lies on the server of connection `id`, over SFTP or FTP.
fn shows_connection(path: &VfsPath, id: ConnectionId) -> bool {
    matches!(
        path.backend,
        BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id }
            if connection_id == id.0
    )
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
