//! The `terminal.*` commands: local, WSL and SSH shells in sessions the engine owns, each
//! belonging to a panel. Which session a panel shows is `terminal_view.rs`; the SSH shells and
//! their login scripts are `ssh_terminal.rs` and `ssh_login.rs` (RC-060-03).

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, MutexGuard, PoisonError},
    time::Duration,
};

use rc_term::{
    InheritedEnv, Integration, Platform, SessionState, ShellChoice, ShellEnv, ShellProfile,
    TermError, TermEvent, TermId, TermSession, TermSize, WslCwd, WslLaunch, choose_distro,
    choose_shell, local_shells, windows_to_wsl_path, wsl_profiles, wsl_program,
};
use rc_vfs::{BackendId, VfsError, VfsPath, WslDistro};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

use super::{
    panel::resolve_path,
    ssh_login::{LoginRunCell, LoginScriptRun},
    ssh_terminal::{server_path, ssh_connection_of},
};
use crate::{
    ConnectionId, Engine, EngineError, EngineEvent, Outcome, PanelId, PanelView, TerminalChange,
    settings::{END_PROGRAMS_KEY, INTEGRATION_KEY},
};

/// How long `terminal.close` waits for the program to end before it answers anyway.
const CLOSE_WAIT: Duration = Duration::from_secs(3);

/// A session and the panel it belongs to.
pub(crate) struct Terminal {
    pub(super) session: Arc<TermSession>,
    pub(super) panel: PanelId,
    /// What a shell on an SSH server has besides.
    pub(super) remote: Option<Remote>,
    /// The clipboard images pasted as paths (`terminal_image.rs`), removed when it goes.
    pub(super) images: Vec<VfsPath>,
}

/// An SSH terminal's connection and its login script's run.
pub(crate) struct Remote {
    pub(super) connection: ConnectionId,
    pub(super) login: LoginRunCell,
}

/// A terminal session as commands answer it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub terminal: TermId,
    /// The panel that shows it.
    pub panel: PanelId,
    pub title: String,
    /// The shell id it runs (`terminal.list` names them).
    pub shell: String,
    /// The session's folder.
    pub cwd: PathBuf,
    pub rows: u16,
    pub cols: u16,
    pub state: SessionState,
    /// The SSH connection of a shell on a server (`terminal.open_ssh`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<ConnectionId>,
    /// The run of the connection's login script, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_script: Option<LoginScriptRun>,
    /// In the answer of `terminal.open` only: the i18n key of a hint for the status line, why a
    /// WSL shell starts in its home and not in the folder (`terminal.wsl.home_*`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    /// Whether it is the session its panel shows in its terminal view (or will show again when
    /// the panel turns from its files back to its terminal).
    pub shown: bool,
}

impl TerminalInfo {
    fn of(entry: &Terminal, shown: bool) -> Self {
        let info = entry.session.info();
        let remote = entry.remote.as_ref();
        Self {
            terminal: info.id,
            panel: entry.panel,
            title: info.title,
            shell: info.shell,
            cwd: info.cwd,
            rows: info.size.rows,
            cols: info.size.cols,
            state: info.state,
            connection_id: remote.map(|remote| remote.connection),
            login_script: remote.and_then(|remote| remote.login.get()),
            notice: None,
            shown,
        }
    }
}

impl Engine {
    /// A terminal session's own handle, for the panel widget that renders it.
    #[must_use]
    pub fn terminal_session(&self, terminal: TermId) -> Option<Arc<TermSession>> {
        self.terminals()
            .get(&terminal)
            .map(|entry| Arc::clone(&entry.session))
    }

    /// The SSH connection a terminal session runs on; `None` for a local or WSL shell, and for
    /// a session that is gone (RC-100-51: its colours are the connection's theme's).
    #[must_use]
    pub fn terminal_connection(&self, terminal: TermId) -> Option<ConnectionId> {
        self.terminals()
            .get(&terminal)
            .and_then(|entry| entry.remote.as_ref().map(|remote| remote.connection))
    }

    /// `terminal.open`: a local or WSL shell in `cwd` (resolved against the panel's folder), or
    /// in the panel's folder. The panel shows it at once, in place of its files or of the session
    /// it showed; an MCP client's terminal appears where the person looks.
    pub(crate) async fn open_terminal(
        &self,
        panel: Option<PanelId>,
        shell: Option<String>,
        distro: Option<String>,
        cwd: Option<&str>,
        size: Option<TermSize>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let folder = self.state().panel(id).path.clone();
        let start = match cwd {
            Some(target) => resolve_path(&folder, target),
            None => folder,
        };
        self.open_terminal_in(id, start, shell, distro, size).await
    }

    /// A local or WSL shell in the folder `start`, shown by `id` (`terminal.open`,
    /// `panel.open_terminal_here`); with the shell integration when the setting says so. Asked
    /// for no shell, a folder in a WSL distribution's share opens that distribution's shell
    /// ([`choose_shell`]). On an SFTP panel's folder it is a shell on that server
    /// (`terminal.open_ssh`, RC-060-03).
    pub(super) async fn open_terminal_in(
        &self,
        id: PanelId,
        start: VfsPath,
        shell: Option<String>,
        distro: Option<String>,
        size: Option<TermSize>,
    ) -> Result<Outcome, EngineError> {
        if let Some(connection) = ssh_connection_of(&start.backend) {
            // An SFTP panel's terminal is a shell on its server, in the panel's folder.
            let folder = server_path(&start.path);
            return self
                .open_ssh_terminal(Some(id), connection, Some(folder), size)
                .await;
        }
        if start.backend != BackendId::Local {
            return Err(EngineError::BackendUnavailable);
        }
        let size = match size {
            Some(size) => TermSize::new(size.rows, size.cols)?,
            None => TermSize::default(),
        };
        let shell = match choose_shell(shell.as_deref(), distro.as_deref(), &start.path) {
            ShellChoice::Wsl(distro) => {
                return self.open_wsl(id, start, distro.as_deref(), size).await;
            }
            ShellChoice::Local(shell) => shell,
        };
        self.require_folder(&start).await?;
        let shells = off_runtime(|| Ok(local_shells())).await?;
        let profile = match shell {
            Some(name) => shells
                .into_iter()
                .find(|profile| profile.id == name)
                .ok_or(TermError::UnknownShell(name))?,
            None => self
                .default_shell(shells)
                .await
                .ok_or_else(|| TermError::UnknownShell(String::new()))?,
        };
        let integration = if self.setting_on(INTEGRATION_KEY).await {
            let zsh_dir = if profile.id == "zsh" {
                off_runtime(|| Ok(zsh_dir())).await?
            } else {
                None
            };
            Integration::local(&profile.id, &InheritedEnv::from_env(), zsh_dir.as_deref())
        } else {
            None
        };
        let end_programs = self.setting_on(END_PROGRAMS_KEY).await;
        let folder = start.path;
        let session = off_runtime(move || {
            TermSession::open_local_with(
                TermId::next(),
                &profile,
                folder,
                size,
                integration.as_ref(),
                end_programs,
            )
        })
        .await?;
        Ok(self.adopt(Arc::new(session), id, None, None))
    }

    /// A WSL shell of `distro` (WSL's default when `None`) in the Linux path of `start`, or in
    /// its home when `start` has none there.
    async fn open_wsl(
        &self,
        panel: PanelId,
        start: VfsPath,
        distro: Option<&str>,
        size: TermSize,
    ) -> Result<Outcome, EngineError> {
        let installed = self.installed_distros().await;
        let names: Vec<String> = installed.iter().map(|known| known.name.clone()).collect();
        let chosen = choose_distro(&names, distro)?;
        let target = chosen.or_else(|| {
            installed
                .iter()
                .find(|known| known.is_default)
                .map(|known| known.name.clone())
        });
        let cwd = windows_to_wsl_path(&start.path, target.as_deref());
        if matches!(cwd, WslCwd::Path(_)) {
            self.require_folder(&start).await?;
        }
        let notice = cwd.notice_key().map(str::to_owned);
        // The default spelled out: the same shell, and the session knows the distribution's
        // share for the folders the shell reports.
        let launch = WslLaunch {
            program: wsl_program(&ShellEnv::from_env()),
            distro: target,
            cwd,
        };
        let integration = if self.setting_on(INTEGRATION_KEY).await {
            Some(Integration::wsl(&InheritedEnv::from_env()))
        } else {
            None
        };
        let end_programs = self.setting_on(END_PROGRAMS_KEY).await;
        let folder = start.path;
        let session = off_runtime(move || {
            TermSession::open_wsl_with(
                TermId::next(),
                &launch,
                folder,
                size,
                integration.as_ref(),
                end_programs,
            )
        })
        .await?;
        Ok(self.adopt(Arc::new(session), panel, notice, None))
    }

    /// The installed WSL distributions; none when they cannot be read.
    async fn installed_distros(&self) -> Vec<WslDistro> {
        self.inner.wsl.distros().await.unwrap_or_else(|error| {
            tracing::warn!(%error, "the WSL distributions cannot be read");
            Vec::new()
        })
    }

    /// Fails unless `start` is an existing folder.
    async fn require_folder(&self, start: &VfsPath) -> Result<(), EngineError> {
        let entry = self.vfs(start)?.stat(&start.path).await?;
        if entry.is_dir_like() {
            Ok(())
        } else {
            Err(VfsError::NotADirectory {
                path: start.path.clone(),
            }
            .into())
        }
    }

    /// Keeps a started session for `panel` and announces it.
    pub(super) fn adopt(
        &self,
        session: Arc<TermSession>,
        panel: PanelId,
        notice: Option<String>,
        remote: Option<Remote>,
    ) -> Outcome {
        self.watch_terminal(&session);
        let terminal = session.id();
        let entry = Terminal {
            session: Arc::clone(&session),
            panel,
            remote,
            images: Vec::new(),
        };
        let info = TerminalInfo::of(&entry, false);
        self.terminals().insert(terminal, entry);
        self.publish_terminal(terminal, TerminalChange::Opened);
        self.set_view(panel, PanelView::Terminal, Some(terminal));
        if session.state().is_exited() {
            // It ended before the watcher could find it in the list.
            self.terminal_ended(terminal);
        }
        Outcome::Terminal(TerminalInfo {
            notice,
            state: session.state(),
            shown: self.shown_terminal(panel) == Some(terminal),
            ..info
        })
    }

    /// `terminal.close`: ends the program, waits a moment for it, forgets the session. A panel
    /// that showed it shows its next running session, or its files.
    pub(crate) async fn close_terminal(&self, terminal: TermId) -> Result<Outcome, EngineError> {
        let removed = self.terminals().remove(&terminal);
        let mut entry = removed.ok_or(TermError::NotFound(terminal))?;
        self.remove_images(std::mem::take(&mut entry.images));
        self.session_gone(terminal, entry.panel);
        entry.session.shutdown();
        if tokio::time::timeout(CLOSE_WAIT, entry.session.wait_exit())
            .await
            .is_err()
        {
            tracing::warn!(%terminal, "the terminal's program did not end in time");
        }
        let info = TerminalInfo::of(&entry, false);
        self.publish_terminal(terminal, TerminalChange::Closed);
        Ok(Outcome::Terminal(info))
    }

    /// `terminal.send_input`.
    pub(crate) fn send_input(
        &self,
        terminal: TermId,
        text: String,
    ) -> Result<Outcome, EngineError> {
        let (session, _) = self.terminal(terminal)?;
        session.write(text.into_bytes())?;
        Ok(Outcome::Terminal(self.info(terminal)?))
    }

    /// `terminal.read_screen`.
    pub(crate) fn read_screen(
        &self,
        terminal: TermId,
        scrollback: usize,
    ) -> Result<Outcome, EngineError> {
        let (session, _) = self.terminal(terminal)?;
        Ok(Outcome::Screen {
            terminal,
            state: session.state(),
            screen: session.screen(scrollback),
        })
    }

    /// `terminal.list`: the sessions, then the local shells and one WSL profile per installed
    /// distribution.
    pub(crate) async fn list_terminals(&self) -> Outcome {
        let terminals = self.infos(|_| true);
        let names: Vec<String> = self
            .installed_distros()
            .await
            .into_iter()
            .map(|known| known.name)
            .collect();
        let mut shells: Vec<ShellProfile> =
            off_runtime(|| Ok(local_shells())).await.unwrap_or_default();
        shells.extend(wsl_profiles(
            Platform::CURRENT,
            &ShellEnv::from_env(),
            &names,
        ));
        Outcome::Terminals { terminals, shells }
    }

    /// The sessions of the panels `wanted` accepts, oldest first.
    pub(super) fn infos(&self, wanted: impl Fn(PanelId) -> bool) -> Vec<TerminalInfo> {
        // Read first: the state and the sessions are never locked together.
        let shown = self.state().shown;
        self.terminals()
            .values()
            .filter(|entry| wanted(entry.panel))
            .map(|entry| {
                let terminal = entry.session.id();
                let shown = shown[entry.panel.index()] == Some(terminal);
                TerminalInfo::of(entry, shown)
            })
            .collect()
    }

    /// A session as commands answer it.
    fn info(&self, terminal: TermId) -> Result<TerminalInfo, EngineError> {
        // Read first: the state and the sessions are never locked together.
        let shown = self.state().shown;
        self.terminals()
            .get(&terminal)
            .map(|entry| TerminalInfo::of(entry, shown[entry.panel.index()] == Some(terminal)))
            .ok_or_else(|| TermError::NotFound(terminal).into())
    }

    pub(super) fn terminals(&self) -> MutexGuard<'_, BTreeMap<TermId, Terminal>> {
        self.inner
            .terminals
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn terminal(
        &self,
        terminal: TermId,
    ) -> Result<(Arc<TermSession>, PanelId), EngineError> {
        self.terminals()
            .get(&terminal)
            .map(|entry| (Arc::clone(&entry.session), entry.panel))
            .ok_or_else(|| TermError::NotFound(terminal).into())
    }

    fn publish_terminal(&self, terminal: TermId, change: TerminalChange) {
        self.publish(EngineEvent::Terminal { terminal, change });
    }

    /// Announces the session's title changes and its end as engine events; after the end its
    /// panel turns away from it. Holds the engine weakly: a session never keeps it alive.
    fn watch_terminal(&self, session: &TermSession) {
        let terminal = session.id();
        let mut events = session.subscribe();
        let exited_already = session.state().is_exited();
        let engine_events = self.inner.events.clone();
        let engine = Arc::downgrade(&self.inner);
        let announce = move |change| {
            // No subscriber is not an error: a headless engine may have none.
            let _ = engine_events.send(EngineEvent::Terminal { terminal, change });
        };
        if exited_already {
            announce(TerminalChange::Exited);
            return;
        }
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(TermEvent::Title(_)) => announce(TerminalChange::Title),
                    Ok(TermEvent::Folder) => announce(TerminalChange::Folder),
                    Ok(TermEvent::Prompt) => announce(TerminalChange::Follow),
                    Ok(TermEvent::Exited(_)) => {
                        announce(TerminalChange::Exited);
                        if let Some(inner) = engine.upgrade() {
                            Engine { inner }.terminal_ended(terminal);
                        }
                        break;
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }
}

/// `start` on the blocking pool (RC-080-46): looking for shells, writing zsh's integration and
/// starting a shell in a pseudo-terminal are system calls that may wait, never on the runtime's
/// threads. The session's pump still runs on the runtime: the blocking pool is inside it.
async fn off_runtime<T: Send + 'static>(
    start: impl FnOnce() -> Result<T, TermError> + Send + 'static,
) -> Result<T, TermError> {
    tokio::task::spawn_blocking(start)
        .await
        .unwrap_or_else(|join| Err(TermError::Pty(join.to_string())))
}

/// The folder zsh's integration lives in (`<data>/shell-integration/zsh`), written when needed;
/// `None` when there is no data folder or it cannot be written.
fn zsh_dir() -> Option<PathBuf> {
    let base = directories::ProjectDirs::from("de", "degoya", "rCommander")?
        .data_local_dir()
        .join("shell-integration");
    rc_term::zsh_dotdir(&base)
        .map_err(|error| tracing::warn!(%error, "zsh's shell integration cannot be written"))
        .ok()
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
