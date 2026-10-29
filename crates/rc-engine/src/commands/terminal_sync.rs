//! The folder between a panel and its terminal (RC-040-04), and `panel.open_terminal_here`.
//!
//! Terminal follows panel (setting `terminal.follow_panel`, on by default): when a panel turns
//! from its files to its terminal, or moves to another folder while it shows the terminal, the
//! shell it shows gets a `cd` - typed by `rc-term` only at an empty marked prompt, kept for the
//! next prompt otherwise ([`rc_term::TermSession::follow`]). Panel follows terminal (setting
//! `terminal.panel_follows`, on by default): when a panel turns back to its files, it shows the
//! folder the shell last reported - only a shell of this machine (PTY, WSL): what a server prints
//! never moves a local panel (RC-080-10). Each attempt is announced as
//! [`TerminalChange::Follow`]; the window reads [`rc_term::TermSession::follow_status`] for the
//! status line.

use std::path::{Path, PathBuf};

use rc_term::{FollowStatus, PromptState, TermId};
use rc_vfs::{BackendId, VfsPath};
use serde::{Deserialize, Serialize};

use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelId, PanelSnapshot, PanelView, TerminalChange,
    settings::{FOLLOW_PANEL_KEY, PANEL_FOLLOWS_KEY},
};

/// The terminal a panel shows, as `panel.get_state` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelTerminal {
    pub terminal: TermId,
    /// The folder the shell last reported; `None` without shell integration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// Whether the shell waits at its prompt (`unknown` without shell integration).
    pub prompt: PromptState,
    /// Why the shell is not in the panel's folder, when it is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<FollowStatus>,
}

impl Engine {
    /// The shell `panel` shows goes to the panel's folder, when the panel shows its terminal and
    /// the setting says so.
    pub(super) async fn terminal_follows_panel(&self, panel: PanelId) {
        let (view, folder) = {
            let state = self.state();
            let shown = state.panel(panel);
            (shown.view, shown.path.clone())
        };
        if view != PanelView::Terminal || folder.backend != BackendId::Local {
            return;
        }
        let Some((terminal, session)) = self.shown_session(panel) else {
            return;
        };
        if !self.setting_on(FOLLOW_PANEL_KEY).await {
            return;
        }
        match session.follow(&folder.path) {
            Ok(_) => self.publish(EngineEvent::Terminal {
                terminal,
                change: TerminalChange::Follow,
            }),
            Err(error) => {
                tracing::debug!(%terminal, %error, "the terminal cannot follow its panel")
            }
        }
    }

    /// `panel` shows the folder its terminal last reported, when that is another one and the
    /// setting says so. A folder that cannot be shown leaves the panel where it is. A shell on an
    /// SSH server is never followed: its output is the server's, and a folder it names - a share
    /// of the attacker's among them - is no folder of this machine (RC-080-10).
    pub(super) async fn panel_follows_terminal(&self, panel: PanelId) {
        let Some((terminal, session)) = self.shown_session(panel) else {
            return;
        };
        if self.is_remote_terminal(terminal) {
            return;
        }
        let Some(reported) = session.reported_folder() else {
            return;
        };
        let current = self.state().panel(panel).path.clone();
        if current.backend == BackendId::Local && same_folder(&current.path, &reported) {
            return;
        }
        if !self.setting_on(PANEL_FOLLOWS_KEY).await {
            return;
        }
        if let Err(error) = self.show_path(panel, VfsPath::local(reported)).await {
            tracing::debug!(%terminal, code = error.code(), "the panel cannot show the shell's folder");
        }
    }

    /// `panel.get_state`: the panel, and for a panel showing its terminal the shell's folder and
    /// prompt.
    pub(crate) fn panel_state(&self, panel: Option<PanelId>) -> PanelSnapshot {
        let mut snapshot = self.snapshot(panel);
        if snapshot.view == PanelView::Terminal
            && let Some((terminal, session)) = self.shown_session(snapshot.panel)
        {
            snapshot.terminal = Some(PanelTerminal {
                terminal,
                folder: session.reported_folder(),
                prompt: session.prompt_state(),
                follow: session.follow_status(),
            });
        }
        snapshot
    }

    /// `panel.open_terminal_here`: a new shell in the folder of `panel` (the distribution's in a
    /// WSL share, else the default profile), shown by that panel or, with `other`, by the other
    /// one.
    pub(crate) async fn open_terminal_here(
        &self,
        panel: Option<PanelId>,
        other: bool,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let folder = self.state().panel(id).path.clone();
        let shown_by = if other { id.other() } else { id };
        self.open_terminal_in(shown_by, folder, None, None, None)
            .await
    }

    /// The session `panel` shows (or will show again) in its terminal view.
    fn shown_session(
        &self,
        panel: PanelId,
    ) -> Option<(TermId, std::sync::Arc<rc_term::TermSession>)> {
        let terminal = self.shown_terminal(panel)?;
        let (session, _) = self.terminal(terminal).ok()?;
        Some((terminal, session))
    }

    /// Whether `terminal` is a shell on an SSH server (`terminal.open_ssh`).
    fn is_remote_terminal(&self, terminal: TermId) -> bool {
        self.terminals()
            .get(&terminal)
            .is_some_and(|entry| entry.remote.is_some())
    }
}

/// Whether two local folders are the same; on Windows ignoring ASCII case and a trailing
/// separator, as the file system does.
fn same_folder(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.to_str(), b.to_str()) {
        (Some(a), Some(b)) if cfg!(windows) => a
            .trim_end_matches('\\')
            .eq_ignore_ascii_case(b.trim_end_matches('\\')),
        _ => false,
    }
}

#[cfg(test)]
#[path = "terminal_sync_tests.rs"]
mod tests;
