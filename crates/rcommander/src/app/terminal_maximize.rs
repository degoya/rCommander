//! The active terminal over the whole window width (RC-040-05): Ctrl+A-umlaut (Ctrl+' on a US
//! layout, the same key) lays the terminal of the active panel over the area of both panels, the
//! same key lays it back.
//!
//! The engine turns a panel showing its files to its terminal first and announces
//! `terminal.maximize` ([`EngineEvent::MaximizeTerminal`]); which panel is maximized is the
//! window's alone and never stored, so a start shows two panels. The other panel is hidden, not
//! closed; the terminal widget fits its grid to the wider area by itself, which resizes the
//! session's PTY - the session, its scrollback and its program run on. The drive bars, the
//! panel's folder tabs, the command line, the function-key bar and the status bar stay. Both
//! panels come back when the panel stops showing its terminal (Ctrl+O-umlaut, its last session
//! closed or ended) and when the other panel becomes the active one - a click cannot reach it,
//! an MCP call can - so the keyboard never goes to a hidden panel.

use rc_engine::{EngineEvent, PanelId, PanelView};

use super::{App, Effect};

impl App {
    /// The panel whose terminal fills the panels' area, while it is the active panel and shows
    /// its terminal.
    pub(super) fn maximized(&self) -> Option<PanelId> {
        self.terms.maximized.filter(|panel| {
            *panel == self.active && self.pane(*panel).state.view == PanelView::Terminal
        })
    }

    /// `terminal.maximize` was announced for `panel`: its terminal over both panels, or both
    /// panels again; the terminal keeps the keyboard either way. Only the active panel is laid
    /// over the other.
    pub(super) fn toggle_maximized(&mut self, panel: PanelId) -> Vec<Effect> {
        if panel != self.active {
            return Vec::new();
        }
        self.terms.maximized = if self.maximized() == Some(panel) {
            None
        } else {
            Some(panel)
        };
        vec![Effect::FocusTerminal(panel)]
    }

    /// After `event`: both panels again when the maximized panel turned from its terminal or
    /// another panel became the active one.
    pub(super) fn maximized_follows(&mut self, event: &EngineEvent) {
        let Some(maximized) = self.terms.maximized else {
            return;
        };
        let back = match event {
            EngineEvent::Panel { panel, .. } => {
                *panel == maximized && self.pane(maximized).state.view != PanelView::Terminal
            }
            EngineEvent::ActivePanel { panel } => *panel != maximized,
            _ => false,
        };
        if back {
            self.terms.maximized = None;
        }
    }
}

#[cfg(test)]
#[path = "terminal_maximize_tests.rs"]
mod tests;
