//! The view part of `workspaces.apply` (RC-100-68): a panel shows its files, or the terminal the
//! workspace names - a running session of the same kind in that panel reused (the same SSH
//! connection, the same WSL distribution, a local shell of the same id), else a new one started
//! in the start folder. A shell that does not start, or a connection deleted since, is left out
//! with a note and the panel shows its files.

use rc_term::{TermId, WSL_SHELL};
use rc_vfs::{BackendId, VfsPath};

use super::{PanelSpec, TermSpec, WorkspaceError, apply::note};
use crate::{Engine, EngineError, PanelId, PanelView, commands::resolve_path};

impl Engine {
    /// Panel `panel` shows what `spec` says: its files or its terminal.
    pub(super) async fn apply_view(
        &self,
        panel: PanelId,
        spec: &PanelSpec,
        skipped: &mut Vec<String>,
    ) {
        match (spec.view, &spec.terminal) {
            (PanelView::Terminal, Some(terminal)) => {
                if let Err(error) = self.show_workspace_terminal(panel, terminal).await {
                    skipped.push(note(error.code(), terminal.describe()));
                    self.show_files(Some(panel)).await;
                }
            }
            _ => {
                self.show_files(Some(panel)).await;
            }
        }
    }

    /// The fitting running session of `panel` shown, else a new one started.
    async fn show_workspace_terminal(
        &self,
        panel: PanelId,
        terminal: &TermSpec,
    ) -> Result<(), EngineError> {
        if let Some(running) = self.fitting_terminal(panel, terminal) {
            self.show_terminal(running).await?;
            return Ok(());
        }
        match terminal {
            TermSpec::Local { shell, cwd } => {
                let start = self.terminal_start(panel, cwd.as_deref());
                self.open_terminal_in(panel, start, shell.clone(), None, None)
                    .await?;
            }
            TermSpec::Wsl { distro, cwd } => {
                let start = self.terminal_start(panel, cwd.as_deref());
                self.open_terminal_in(panel, start, None, Some(distro.clone()), None)
                    .await?;
            }
            TermSpec::Ssh { connection_id, cwd } => {
                if !self.connection_exists(*connection_id).await {
                    return Err(WorkspaceError::Connection { id: *connection_id }.into());
                }
                self.open_ssh_terminal(Some(panel), *connection_id, cwd.clone(), None)
                    .await?;
            }
        }
        Ok(())
    }

    /// The newest running session of `panel` of the kind `terminal` names.
    fn fitting_terminal(&self, panel: PanelId, terminal: &TermSpec) -> Option<TermId> {
        self.panel_terminals(panel)
            .into_iter()
            .rev()
            .filter(|info| !info.state.is_exited())
            .find(|info| match terminal {
                TermSpec::Ssh { connection_id, .. } => info.connection_id == Some(*connection_id),
                TermSpec::Wsl { distro, .. } => {
                    info.connection_id.is_none()
                        && self
                            .terminal_distro(info.terminal)
                            .is_some_and(|known| known.eq_ignore_ascii_case(distro))
                }
                TermSpec::Local { shell, .. } => {
                    info.connection_id.is_none()
                        && info.shell != WSL_SHELL
                        && self.terminal_distro(info.terminal).is_none()
                        && shell.as_ref().is_none_or(|shell| *shell == info.shell)
                }
            })
            .map(|info| info.terminal)
    }

    /// Where a local or WSL shell of `panel` starts: `cwd` against the panel's folder when that
    /// is on this machine, else `cwd` as it is; without `cwd` the panel's folder, or the home
    /// folder when the panel is on a server (a local shell never starts there).
    fn terminal_start(&self, panel: PanelId, cwd: Option<&str>) -> VfsPath {
        let folder = self.state().panel(panel).path.clone();
        let local = folder.backend == BackendId::Local;
        match (cwd, local) {
            (Some(cwd), true) => resolve_path(&folder, cwd),
            (Some(cwd), false) => VfsPath::local(cwd),
            (None, true) => folder,
            (None, false) => VfsPath::local(home_folder()),
        }
    }
}

/// The person's home folder; the root when the system names none.
fn home_folder() -> std::path::PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(std::path::MAIN_SEPARATOR_STR))
}
