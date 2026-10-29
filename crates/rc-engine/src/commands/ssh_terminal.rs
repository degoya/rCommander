//! `terminal.open_ssh` (RC-060-03): a shell on an SSH server in a panel's terminal, beside the
//! local and WSL shells of `terminal.rs`.
//!
//! The terminal is a channel of the connection's pooled session ([`Engine::ssh_channel`]), so an
//! SFTP panel and a terminal on one server share a TCP connection; the channel gets a PTY of the
//! terminal's size with `TERM=xterm-256color` and a shell, and `rc_term::spawn_ssh` drives it -
//! window-change on a resize, the terminal's end with the channel's or the connection's. A
//! connection with a login script runs it first (`ssh_login.rs`). A start folder - the SFTP
//! panel's, the one asked for, else the connection's own - is a `cd` typed into the shell, quoted
//! for a POSIX shell, after the script.

use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use rc_remote::ssh::start_pty_shell;
use rc_term::{DEFAULT_SCROLLBACK, SessionOptions, TermId, TermSession, TermSize, spawn_ssh, tap};
use rc_vfs::{BackendId, VfsError};

use super::{ssh_login::LoginRunCell, terminal::Remote};
use crate::{
    ConnectionId, ConnectionKind, Engine, EngineError, LoginScriptId, Outcome, PanelId,
    SessionChannel,
};

/// The terminal type the server is told.
pub const SSH_TERM: &str = "xterm-256color";

/// The shell id of an SSH terminal in `terminal.list`.
pub const SSH_SHELL: &str = "ssh";

impl Engine {
    /// `terminal.open_ssh`: a shell on the server of `connection` in `panel` (the active one by
    /// default), which shows it at once.
    pub(crate) async fn open_ssh_terminal(
        &self,
        panel: Option<PanelId>,
        connection: ConnectionId,
        cwd: Option<String>,
        size: Option<TermSize>,
    ) -> Result<Outcome, EngineError> {
        let panel = self.resolve(panel);
        let size = match size {
            Some(size) => TermSize::new(size.rows, size.cols)?,
            None => TermSize::default(),
        };
        let record = self.db()?.connection(connection).await?;
        if record.kind != ConnectionKind::Ssh {
            return Err(EngineError::ConnectionNotSsh { id: connection });
        }
        let cwd = cwd
            .or(record.start_directory)
            .filter(|folder| !folder.trim().is_empty());
        // Refused before anything reaches the server.
        let then = cwd.as_deref().map(cd_command).transpose()?;
        let login = match record.login_script_id {
            Some(script) => Some(
                self.prepare_login(LoginScriptId(script), connection)
                    .await?,
            ),
            None => None,
        };
        let SessionChannel { mut channel, lease } = self.ssh_channel(connection).await?;
        start_pty_shell(
            &mut channel,
            SSH_TERM,
            u32::from(size.cols),
            u32::from(size.rows),
        )
        .await?;
        let spawned = spawn_ssh(channel, lease);
        let (spawned, tapped) = match login {
            Some(_) => {
                let (spawned, tapped) = tap(spawned);
                (spawned, Some(tapped))
            }
            None => (spawned, None),
        };
        let session = Arc::new(TermSession::start(
            TermId::next(),
            spawned,
            SessionOptions {
                title: record.name,
                shell: SSH_SHELL.to_owned(),
                cwd: PathBuf::from(cwd.as_deref().unwrap_or("~")),
                size,
                scrollback: DEFAULT_SCROLLBACK,
            },
        ));
        let cell = LoginRunCell::default();
        match (login, tapped) {
            (Some(login), Some(tapped)) => {
                // Held before anyone else sees the session: no key reaches the server first.
                self.start_login(&session, tapped, login, &cell, then);
            }
            _ => {
                if let Some(then) = then {
                    session.write(then)?;
                }
            }
        }
        let remote = Remote {
            connection,
            login: cell,
        };
        Ok(self.adopt(session, panel, None, Some(remote)))
    }
}

/// The SSH connection whose server a panel on `backend` lists: an SFTP panel's.
pub(crate) fn ssh_connection_of(backend: &BackendId) -> Option<ConnectionId> {
    match backend {
        BackendId::Sftp { connection_id } => Some(ConnectionId(*connection_id)),
        _ => None,
    }
}

/// An SFTP panel's folder as the server spells it: the components joined with `/`, so a path
/// Windows joined with `\` still names the server's folder.
pub(crate) fn server_path(path: &Path) -> String {
    let mut text = String::new();
    for component in path.components() {
        match component {
            Component::RootDir => text.push('/'),
            Component::Normal(name) => {
                if !text.is_empty() && !text.ends_with('/') {
                    text.push('/');
                }
                text.push_str(&name.to_string_lossy());
            }
            Component::ParentDir => {
                if !text.is_empty() && !text.ends_with('/') {
                    text.push('/');
                }
                text.push_str("..");
            }
            Component::CurDir | Component::Prefix(_) => {}
        }
    }
    if text.is_empty() {
        text.push('/');
    }
    text
}

/// `cd` into `folder` and Enter, the folder quoted for a POSIX shell: in single quotes, each `'`
/// in it as `'\''`, so spaces, quotes, `$` and backslashes arrive as they are.
///
/// # Errors
///
/// `invalid_path` for a folder with a control character (CR, LF, ESC, Ctrl+C, DEL, C1): no
/// quoting holds one back on an interactive line - it would end the line early or press keys of
/// its own - so nothing is typed.
pub fn cd_command(folder: &str) -> Result<Vec<u8>, EngineError> {
    if folder.chars().any(char::is_control) {
        return Err(VfsError::InvalidPath {
            path: PathBuf::from(folder.escape_debug().to_string()),
            reason: "a control character cannot be typed into the shell",
        }
        .into());
    }
    Ok(format!("cd {}\r", shell_quote(folder)).into_bytes())
}

/// `text` as one word of a POSIX shell.
#[must_use]
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
#[path = "ssh_terminal_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ssh_terminal_login_tests.rs"]
mod login_tests;

#[cfg(test)]
#[path = "ssh_terminal_follow_tests.rs"]
mod follow_tests;

#[cfg(test)]
#[path = "ssh_terminal_fault_tests.rs"]
mod fault_tests;
