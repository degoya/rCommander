//! `workspaces.capture` (RC-100-68): what both panels show now, as a workspace - each panel's
//! tabs with folder, sort, view and lock, the tab it shows, the file list or the terminal it shows
//! (a local shell, a WSL distribution's or an SSH connection's, with its folder), the active panel
//! and the divider. Saved as a new workspace, or over the spec of a stored one.
//!
//! A tab inside an archive becomes the folder that holds the archive file: a workspace names
//! folders on this machine or on a server, never inside an archive. A panel keeps its first
//! [`MAX_TABS`] tabs.

use rc_vfs::{BackendId, VfsPath};
use serde_json::Value;

use super::{
    MAX_TABS, PanelSpec, SPEC_VERSION, TabSpec, TermSpec, WorkspaceError, WorkspaceId,
    WorkspaceSpec, WorkspacesOutcome,
    spec::{is_plain, is_text},
    store::answer,
};
use crate::{
    CreateWorkspaceRequest, Engine, EngineError, Outcome, PanelId, PanelView, TerminalInfo,
    UpdateWorkspaceRequest,
    settings::{PANEL_SPLIT_KEY, stored_setting},
};

/// A panel's tabs as the capture reads them under the state lock.
struct PanelNow {
    tabs: Vec<TabSpec>,
    active_tab: usize,
    view: PanelView,
}

/// A location a workspace can name: itself on this machine or a server, the folder holding the
/// archive file inside an archive; `None` for any other backend.
fn plain_location(location: &VfsPath) -> Option<VfsPath> {
    match &location.backend {
        BackendId::Local | BackendId::Sftp { .. } | BackendId::Ftp { .. } => Some(location.clone()),
        BackendId::Archive { outer, .. } => {
            outer.parent().and_then(|folder| plain_location(&folder))
        }
        _ => None,
    }
}

impl Engine {
    /// `workspaces.capture`: the panels as they are, over the spec of the workspace `id` (and
    /// its name when `name` is given), or as a new workspace named `name`.
    pub(crate) async fn capture_workspace(
        &self,
        id: Option<WorkspaceId>,
        name: Option<String>,
    ) -> Result<Outcome, EngineError> {
        let spec = self.current_workspace_spec().await?;
        match id {
            Some(id) => {
                self.update_workspace(UpdateWorkspaceRequest {
                    id,
                    name,
                    spec: Some(spec),
                    ..UpdateWorkspaceRequest::default()
                })
                .await?;
                Ok(answer(WorkspacesOutcome::Saved { id }))
            }
            None => {
                let name = name.ok_or_else(|| WorkspaceError::Name {
                    reason: "a new workspace needs a name".into(),
                })?;
                self.create_workspace(CreateWorkspaceRequest {
                    name,
                    spec,
                    theme_light: None,
                    theme_dark: None,
                })
                .await
            }
        }
    }

    /// What both panels show now as a workspace spec, not saved: what "Take current state" of
    /// the workspaces' window fills its form with.
    ///
    /// # Errors
    ///
    /// `workspace_tabs` when a panel has no tab a workspace can name (all in another backend).
    pub async fn current_workspace_spec(&self) -> Result<WorkspaceSpec, EngineError> {
        let (panels, active) = {
            let mut guard = self.state();
            let state = &mut *guard;
            let panels = PanelId::BOTH.map(|panel| {
                let live = match panel {
                    PanelId::Left => &state.left,
                    PanelId::Right => &state.right,
                };
                state.tabs.sync(panel, live);
                let set = state.tabs.set(panel);
                let mut tabs = Vec::new();
                let mut active_tab = 0;
                for (index, tab) in set.tabs().iter().enumerate() {
                    let Some(location) = plain_location(&tab.path) else {
                        continue;
                    };
                    if tabs.len() == MAX_TABS {
                        break;
                    }
                    if index == set.active() {
                        active_tab = tabs.len();
                    }
                    tabs.push(TabSpec {
                        location,
                        sort: Some(tab.sort),
                        mode: Some(tab.mode),
                        locked: tab.locked,
                    });
                }
                PanelNow {
                    tabs,
                    active_tab,
                    view: live.view,
                }
            });
            (panels, state.active)
        };
        let [left, right] = panels;
        let split = match self.db() {
            Ok(db) => stored_setting(db, PANEL_SPLIT_KEY)
                .await
                .as_ref()
                .and_then(Value::as_u64)
                .and_then(|split| u16::try_from(split).ok()),
            Err(_) => None,
        };
        Ok(WorkspaceSpec {
            version: SPEC_VERSION,
            active,
            split_permille: split,
            left: self.panel_spec(PanelId::Left, left)?,
            right: self.panel_spec(PanelId::Right, right)?,
        })
    }

    /// The spec of `panel` from what the capture read: its terminal is the session it shows.
    fn panel_spec(&self, panel: PanelId, now: PanelNow) -> Result<PanelSpec, EngineError> {
        if now.tabs.is_empty() {
            return Err(WorkspaceError::Tabs {
                reason: "the panel has no tab a workspace can name".into(),
            }
            .into());
        }
        let shown = self.shown_terminal(panel);
        let terminal = self
            .panel_terminals(panel)
            .into_iter()
            .find(|info| Some(info.terminal) == shown)
            .map(|info| self.term_spec(&info));
        let view = match (&now.view, &terminal) {
            (PanelView::Terminal, Some(_)) => PanelView::Terminal,
            _ => PanelView::Files,
        };
        Ok(PanelSpec {
            tabs: now.tabs,
            active_tab: now.active_tab,
            view,
            terminal: terminal.filter(|_| view == PanelView::Terminal),
        })
    }

    /// The terminal spec of a running session: its connection's, its distribution's, or its
    /// local shell's, with the folder it is in.
    fn term_spec(&self, info: &TerminalInfo) -> TermSpec {
        let cwd = Some(info.cwd.display().to_string()).filter(|cwd| is_text(cwd) && cwd != "~");
        if let Some(connection_id) = info.connection_id {
            return TermSpec::Ssh { connection_id, cwd };
        }
        if let Some(distro) = self.terminal_distro(info.terminal).filter(|d| is_plain(d)) {
            return TermSpec::Wsl { distro, cwd };
        }
        TermSpec::Local {
            shell: Some(info.shell.clone()).filter(|shell| is_plain(shell)),
            cwd,
        }
    }
}
