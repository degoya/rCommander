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
                let mut effects = self.terminal_wanted(panel, change == PanelChange::View);
                // The panel's trees follow it (RC-100-88).
                effects.extend(self.tree_follows_panel(panel, change));
                effects
            }
            EngineEvent::ActivePanel { panel } => {
                self.active = panel;
                vec![Effect::FocusTerminal(panel)]
            }
            EngineEvent::Terminal { terminal, change } => self.terminal_changed(terminal, change),
            EngineEvent::HotlistChanged => self.hotlist_changed(),
            // A workspace changed or was applied: the list is read again (RC-100-68).
            EngineEvent::WorkspacesChanged => vec![Effect::Task(self.workspaces_task())],
            EngineEvent::WorkspaceManager => self.open_workspaces(),
            // A column set changed: the lists, the palette and the Columns tab read them again
            // (RC-100-90).
            EngineEvent::ColumnSetsChanged => vec![Effect::Task(self.column_sets_task())],
            // The own commands and the Start menu (RC-100-86): read again, their window, the
            // question for a program's parameters, a row only the window runs, and a start as
            // administrator the person said no to - no error, a line in the status bar.
            EngineEvent::UserCommandsChanged | EngineEvent::StartMenuChanged => {
                vec![Effect::Task(self.user_commands_task())]
            }
            EngineEvent::UserCommandsWindow { start_menu } => self.open_user_commands(start_menu),
            // The internal associations' window (RC-100-91).
            EngineEvent::AssociationsWindow => self.open_associations(),
            EngineEvent::UserCommandAsk(ref request) => self.ask_params((**request).clone()),
            EngineEvent::WindowRow { ref name } => self.window_row(name),
            EngineEvent::RunAsAdminCancelled => {
                self.info = Some(t!("user_commands.admin_cancelled").to_owned());
                Vec::new()
            }
            // The button bars (RC-100-87): read again, their window, the keyboard into a bar.
            EngineEvent::ButtonBarsChanged
            | EngineEvent::ButtonBarWindow
            | EngineEvent::ButtonBarFocus { .. } => self.button_bar_event(&event),
            EngineEvent::MaximizeTerminal { panel } => self.toggle_maximized(panel),
            EngineEvent::PasteImage { terminal } => self.paste_image(terminal),
            // A long listing broke off after it was shown: the panel keeps what arrived.
            EngineEvent::ListingFailed { panel, ref code } => {
                self.pane_mut(panel).error = Some(crate::i18n::error_text(code));
                Vec::new()
            }
            // A selection read from a file or the clipboard (RC-100-92) says what it found.
            EngineEvent::SelectionLoaded {
                selected, missing, ..
            } => {
                self.info = Some(t!(
                    "selection.loaded",
                    selected = selected.to_string(),
                    missing = missing.to_string()
                ));
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
            effects.extend(self.tree_follows_panel(panel, PanelChange::Navigated));
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
        // A cursor step, a selection or git's marks change no tab: the tab row is not built
        // again for each (RC-080-45, RC-100-97).
        if !matches!(
            change,
            Some(PanelChange::Cursor | PanelChange::Selection | PanelChange::Git)
        ) {
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
