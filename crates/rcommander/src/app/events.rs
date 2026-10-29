//! What the engine announces, and the terminals that follow a panel's view (and the terminal
//! over the whole window, `terminal_maximize.rs`).
//!
//! A panel's view (files or terminal) and the session it shows are the engine's; the widget that
//! draws the session is the window's (`terminal_pane.rs`). When a panel turns to its terminal, or
//! to another of its sessions, the window makes the widget and gives it the keyboard.

use rc_engine::{EngineEvent, PanelChange, PanelId, PanelView};

use super::{App, Effect};
use crate::i18n::t;

impl App {
    /// A change the engine announced: the panel copies the engine's state.
    pub(super) fn on_engine(&mut self, event: EngineEvent) -> Vec<Effect> {
        // Other events (from later jobs) concern no panel: they are ignored here.
        let effects = match event {
            EngineEvent::Panel { panel, change } => {
                self.refresh(panel, Some(change));
                self.terminal_wanted(panel, change == PanelChange::View)
            }
            EngineEvent::ActivePanel { panel } => {
                self.active = panel;
                vec![Effect::FocusTerminal(panel)]
            }
            EngineEvent::Terminal { terminal, change } => self.terminal_changed(terminal, change),
            EngineEvent::HotlistChanged => self.hotlist_changed(),
            EngineEvent::MaximizeTerminal { panel } => self.toggle_maximized(panel),
            EngineEvent::PasteImage { terminal } => self.paste_image(terminal),
            // A long listing broke off after it was shown: the panel keeps what arrived.
            EngineEvent::ListingFailed { panel, ref code } => {
                self.pane_mut(panel).error = Some(crate::i18n::error_text(code));
                Vec::new()
            }
            // A key trusted, removed or imported: the settings' Host keys page reads it again.
            EngineEvent::KnownHostsChanged => self.trust_store_changed(&event),
            // Plain FTP sends the password in clear: said each time a session comes up.
            EngineEvent::PlainFtp { ref name, .. } => {
                self.info = Some(t!("ftp.plain_warning", name = name));
                Vec::new()
            }
            _ => Vec::new(),
        };
        // After the panel copied the engine's state: a terminal that left the panel, or a panel
        // that left the keyboard, lays the panels side by side again.
        self.maximized_follows(&event);
        effects
    }

    /// After missed events: every panel and the active one, read again.
    pub(super) fn reload(&mut self) -> Vec<Effect> {
        self.active = self.engine.active_panel();
        let mut effects = Vec::new();
        for panel in PanelId::BOTH {
            self.refresh(panel, None);
            effects.extend(self.terminal_wanted(panel, false));
        }
        effects
    }

    /// `panel` reads the engine's state again after `change` (`None`: after missed events).
    fn refresh(&mut self, panel: PanelId, change: Option<PanelChange>) {
        let state = self.engine.panel(panel);
        let pane = self.pane_mut(panel);
        match change {
            Some(PanelChange::Cursor) => pane.show_cursor(state),
            // After missed events a panel may stand in another folder: it is shown as navigated
            // there, without the old folder's message, scroll position and `..` row (RC-080-53).
            _ => pane.show(state, matches!(change, Some(PanelChange::Navigated) | None)),
        }
        // A cursor step or a selection changes no tab: the tab row is not built again for each
        // (RC-080-45).
        if !matches!(change, Some(PanelChange::Cursor | PanelChange::Selection)) {
            self.refresh_tabs(panel);
        }
    }

    /// The terminal to show for `panel`: when its view just changed to a terminal or another
    /// session, or it shows a session the window has no widget for yet.
    fn terminal_wanted(&self, panel: PanelId, turned: bool) -> Vec<Effect> {
        if self.pane(panel).state.view != PanelView::Terminal {
            return Vec::new();
        }
        let missing = self
            .shown_terminal(panel)
            .is_some_and(|terminal| !self.terms.widgets.contains_key(&terminal));
        if turned || missing {
            vec![Effect::ShowTerminal(panel)]
        } else {
            Vec::new()
        }
    }
}
