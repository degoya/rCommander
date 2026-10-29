//! Which terminal a panel shows: `panel.toggle_terminal`, `panel.show_files`, `terminal.show`,
//! `terminal.maximize`, and what a panel shows after its shell ended or was closed.
//!
//! A panel shows its files or one of its terminal sessions ([`PanelView`]). The session it shows
//! is kept while it shows its files, so the toggle returns to the same shell; when that session
//! ends or is closed, the panel shows its newest other running session, or its files when it has
//! none. Turning to the terminal sends the shell to the panel's folder, turning back shows the
//! shell's folder in the panel, each as its setting says (`terminal_sync.rs`, RC-040-04).

use rc_term::{TermError, TermId};
use rc_vfs::{BackendId, VfsPath};

use super::terminal::Terminal;
use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, PanelView, TerminalInfo,
};

impl Engine {
    /// The session `panel` shows in its terminal view, or will show when it turns to it again.
    #[must_use]
    pub fn shown_terminal(&self, panel: PanelId) -> Option<TermId> {
        self.state().shown[panel.index()]
    }

    /// The terminal sessions of `panel`, oldest first: the tabs of its terminal view.
    #[must_use]
    pub fn panel_terminals(&self, panel: PanelId) -> Vec<TerminalInfo> {
        self.infos(|owner| owner == panel)
    }

    /// `panel.toggle_terminal`: a panel showing its files shows its terminal - the session it
    /// showed if that still runs and fits the folder, else its newest running one that fits,
    /// else a new shell in the panel's folder (the distribution's in a WSL share, else the
    /// default profile; the sessions that do not fit stay as tabs); a panel showing its
    /// terminal shows its files, and the shell keeps running.
    pub(crate) async fn toggle_terminal(
        &self,
        panel: Option<PanelId>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        if self.state().panel(id).view == PanelView::Terminal {
            return Ok(self.show_files(Some(id)).await);
        }
        let folder = self.state().panel(id).path.clone();
        match self.running_session(id, |entry| fits(entry, &folder)) {
            Some(terminal) => {
                self.set_view(id, PanelView::Terminal, Some(terminal));
                self.terminal_follows_panel(id).await;
                Ok(Outcome::Panel(self.snapshot(Some(id))))
            }
            None => {
                self.open_terminal_in(id, folder, None, None, None).await?;
                Ok(Outcome::Panel(self.snapshot(Some(id))))
            }
        }
    }

    /// `terminal.maximize` (RC-040-05): the window is asked to lay the panel's terminal over both
    /// panels, or to show both again. A panel showing its files turns to its terminal first, as
    /// `panel.toggle_terminal` does (a shell starts when none fits); the toggle is the window's.
    pub(crate) async fn maximize_terminal(
        &self,
        panel: Option<PanelId>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        if self.state().panel(id).view != PanelView::Terminal {
            self.toggle_terminal(Some(id)).await?;
        }
        self.publish(EngineEvent::MaximizeTerminal { panel: id });
        Ok(Outcome::Panel(self.snapshot(Some(id))))
    }

    /// `panel.show_files`: the panel shows its files - in the folder its shell last reported,
    /// when it follows the terminal; its terminal keeps running.
    pub(crate) async fn show_files(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        let was_terminal = self.state().panel(id).view == PanelView::Terminal;
        self.set_view(id, PanelView::Files, None);
        if was_terminal {
            self.panel_follows_terminal(id).await;
        }
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// `terminal.show`: the panel the session belongs to shows it.
    ///
    /// # Errors
    ///
    /// `terminal_not_found`, or `terminal_exited` for a session whose program has ended.
    pub(crate) async fn show_terminal(&self, terminal: TermId) -> Result<Outcome, EngineError> {
        let (session, panel) = self.terminal(terminal)?;
        if session.state().is_exited() {
            return Err(TermError::Exited.into());
        }
        let turned = self.state().panel(panel).view == PanelView::Files;
        self.set_view(panel, PanelView::Terminal, Some(terminal));
        if turned {
            // Only a panel turning from its files: a tab switch leaves each shell where it is.
            self.terminal_follows_panel(panel).await;
        }
        Ok(Outcome::Panel(self.snapshot(Some(panel))))
    }

    /// `panel` shows `view`, and `terminal` in its terminal view when one is given.
    pub(super) fn set_view(
        &self,
        id: PanelId,
        view: PanelView,
        terminal: Option<TermId>,
    ) -> Outcome {
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            if terminal.is_some() {
                state.shown[id.index()] = terminal;
            }
            let panel = state.panel_mut(id);
            panel.view = view;
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::View,
        });
        Outcome::Panel(snapshot)
    }

    /// The program of `terminal` ended: its panel turns away from it.
    pub(super) fn terminal_ended(&self, terminal: TermId) {
        let ended = self
            .terminals()
            .get_mut(&terminal)
            .map(|entry| (entry.panel, std::mem::take(&mut entry.images)));
        let Some((panel, images)) = ended else {
            // Closed already; the close turned the panel.
            return;
        };
        self.remove_images(images);
        self.session_gone(terminal, panel);
    }

    /// After `terminal` of `panel` ended or was closed: a panel that showed it shows its newest
    /// other running session, or its files when it has none.
    pub(super) fn session_gone(&self, terminal: TermId, panel: PanelId) {
        if self.shown_terminal(panel) != Some(terminal) {
            return;
        }
        let next = self.running_session(panel, |entry| entry.session.id() != terminal);
        {
            let mut state = self.state();
            if state.shown[panel.index()] != Some(terminal) {
                // Another command chose a session meanwhile.
                return;
            }
            state.shown[panel.index()] = next;
            if next.is_none() {
                state.panel_mut(panel).view = PanelView::Files;
            }
        }
        self.publish(EngineEvent::Panel {
            panel,
            change: PanelChange::View,
        });
    }

    /// The session `panel` should show: the one it shows while that runs, else its newest
    /// running one; only sessions `wanted` accepts.
    fn running_session(
        &self,
        panel: PanelId,
        wanted: impl Fn(&Terminal) -> bool,
    ) -> Option<TermId> {
        let shown = self.shown_terminal(panel);
        let running: Vec<TermId> = self
            .terminals()
            .values()
            .filter(|entry| entry.panel == panel && !entry.session.state().is_exited())
            .filter(|&entry| wanted(entry))
            .map(|entry| entry.session.id())
            .collect();
        shown
            .filter(|terminal| running.contains(terminal))
            .or_else(|| running.last().copied())
    }
}

/// Whether the toggle may show `entry` for the panel folder `folder`: a local or WSL shell only
/// in a folder of its own system (or the one it is in), so a PowerShell tab is not reused in a
/// WSL share nor a WSL shell on a Windows folder - a fitting shell opens beside it. An SSH shell
/// and a panel on a server keep the old rule: any running session.
fn fits(entry: &Terminal, folder: &VfsPath) -> bool {
    folder.backend != BackendId::Local
        || entry.remote.is_some()
        || entry.session.fits_folder(&folder.path)
}

#[cfg(test)]
#[path = "terminal_view_tests.rs"]
mod tests;
