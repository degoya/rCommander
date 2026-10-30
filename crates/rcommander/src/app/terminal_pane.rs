//! A panel's terminal in the window: the widgets of the sessions the panels show, the bar of a
//! panel's sessions, the shell picker and the question before a multi-line paste.
//!
//! The sessions are the engine's: which one a panel shows is `shown_terminal`, and every change
//! (a tab clicked, a tab closed, a shell picked) is a command (`terminal.show`,
//! `terminal.close`, `terminal.open`). The window keeps only the widget per session, which draws
//! the session's `Term` and types into it directly. A session whose shell ended is closed by the
//! window, its exit code in the panel's status line.
//!
//! An SSH terminal's login script (RC-060-03) reports in the status line as it runs; a run that
//! does not get through also says so in the terminal, and the shell is the person's after it.
//!
//! Keys: while a terminal has the keyboard every key is the shell's but the toggle,
//! Ctrl+O-umlaut or Ctrl+` (both by place), and the terminal over the whole window, Ctrl+A-umlaut or Ctrl+'
//! (`always` in the keymap, the widget's reserved keys); while the picker
//! or the paste question is open every key is theirs. Ctrl+Shift+O-umlaut (Ctrl+Shift+` on a US
//! layout: `terminal.open` without a shell) asks for the shell first when there is more than one.

use std::collections::BTreeMap;

use iced::Task;
use rc_engine::{
    Chord, Command, FollowStatus, LoginRunState, PanelId, PanelView, SessionState, TermId,
    TerminalChange,
};
use rc_term_widget::{CopySettings, Event as WidgetEvent, Terminal, TerminalView};
use rc_vfs::BackendId;

use super::{App, Effect, Message};
use crate::{
    i18n::{error_text, t},
    term, theme,
};

/// What the terminal parts of the window send.
#[derive(Debug, Clone)]
pub enum TermMessage {
    /// From a terminal widget: a redraw, a paste to confirm.
    Widget(WidgetEvent),
    /// A tab of a panel's bar was clicked.
    Select(PanelId, TermId),
    /// The close mark of a tab.
    Close(PanelId, TermId),
    /// The bar's `+`: choose a shell for a new terminal in the panel.
    Choose(PanelId),
    /// A shell of the picker, by its place in [`term::shells`].
    Pick(usize),
    CancelPick,
    PasteConfirmed,
    PasteCancelled,
    /// The stored copy settings (`terminal_copy.rs`).
    Copy(CopySettings),
    /// A step of pasting the clipboard's image as a path (`terminal_image.rs`).
    Image(super::terminal_image::ImageMsg),
}

/// The shell picker: for which panel, and the row the arrows stand on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picker {
    pub panel: PanelId,
    pub selected: usize,
}

/// The window's side of the terminals.
#[derive(Default)]
pub struct TermPanes {
    /// One widget per session a panel showed, until the session is closed.
    pub(super) widgets: BTreeMap<TermId, Terminal>,
    pub picker: Option<Picker>,
    /// Text with a line break waiting for a yes, and the session it is for.
    pub paste: Option<(TermId, String)>,
    /// The panel whose terminal is laid over both panels (`terminal_maximize.rs`).
    pub maximized: Option<PanelId>,
    /// How the widgets copy (`terminal_copy.rs`).
    pub(super) copy: CopySettings,
}

impl TermPanes {
    fn dialog_open(&self) -> bool {
        self.picker.is_some() || self.paste.is_some()
    }
}

/// A widget event as the window's message.
pub(super) fn widget_message(event: WidgetEvent) -> Message {
    Message::Terminal(TermMessage::Widget(event))
}

impl App {
    /// The session `panel` shows in its terminal view, when it shows one.
    pub(super) fn shown_terminal(&self, panel: PanelId) -> Option<TermId> {
        if self.pane(panel).state.view == PanelView::Terminal {
            self.engine.shown_terminal(panel)
        } else {
            None
        }
    }

    pub(super) fn on_terminal(&mut self, message: TermMessage) -> Vec<Effect> {
        match message {
            TermMessage::Widget(WidgetEvent::Redraw(terminal)) => {
                if let Some(widget) = self.terms.widgets.get(&terminal) {
                    widget.redraw();
                }
            }
            TermMessage::Widget(WidgetEvent::Paste(terminal, text)) => {
                self.terms.paste = Some((terminal, text));
            }
            TermMessage::Widget(WidgetEvent::Copy(_, text)) if self.terms.copy.osc52 => {
                return vec![Effect::Task(iced::clipboard::write(text))];
            }
            TermMessage::Widget(WidgetEvent::Copy(..)) => {}
            TermMessage::Select(panel, terminal) => {
                return vec![Effect::Dispatch(
                    panel,
                    vec![Command::ShowTerminal { terminal }],
                )];
            }
            TermMessage::Close(panel, terminal) => {
                return vec![Effect::Dispatch(
                    panel,
                    vec![Command::CloseTerminal { terminal }],
                )];
            }
            TermMessage::Choose(panel) => {
                // A server panel's terminal is a shell on that server: nothing to choose.
                if self.pane(panel).state.path.backend != BackendId::Local {
                    let open = Command::OpenTerminal {
                        panel: Some(panel),
                        shell: None,
                        distro: None,
                        cwd: None,
                        size: None,
                    };
                    return vec![Effect::Dispatch(panel, vec![open])];
                }
                self.terms.picker = Some(Picker { panel, selected: 0 });
            }
            TermMessage::Pick(index) => return self.pick(index),
            TermMessage::Copy(copy) => self.set_terminal_copy(copy),
            TermMessage::Image(message) => return self.on_image(message),
            TermMessage::CancelPick => {
                if let Some(picker) = self.terms.picker.take() {
                    return vec![Effect::FocusTerminal(picker.panel)];
                }
            }
            TermMessage::PasteConfirmed => {
                if let Some((terminal, text)) = self.terms.paste.take() {
                    if let Some(widget) = self.terms.widgets.get(&terminal) {
                        widget.paste(&text);
                    }
                    return vec![Effect::FocusTerminal(self.active)];
                }
            }
            TermMessage::PasteCancelled => {
                if self.terms.paste.take().is_some() {
                    return vec![Effect::FocusTerminal(self.active)];
                }
            }
        }
        Vec::new()
    }

    /// The shell at `index` of the picker starts in the picker's panel, which shows it.
    fn pick(&mut self, index: usize) -> Vec<Effect> {
        let Some(picker) = self.terms.picker.take() else {
            return Vec::new();
        };
        let Some(profile) = term::shells().get(index) else {
            return Vec::new();
        };
        let open = Command::OpenTerminal {
            panel: Some(picker.panel),
            shell: Some(profile.id.clone()),
            distro: profile.distro.clone(),
            cwd: None,
            size: None,
        };
        vec![Effect::Dispatch(picker.panel, vec![open])]
    }

    /// A key the terminal parts take before the keymap: every key while the picker or the paste
    /// question is open, and Ctrl+Shift+O-umlaut when there is a shell to choose. `None` leaves
    /// it to the keymap.
    pub(super) fn terminal_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        let plain = chord.is_plain();
        if self.terms.paste.is_some() {
            let message = match chord.key.as_str() {
                "Enter" if plain => TermMessage::PasteConfirmed,
                "Escape" => TermMessage::PasteCancelled,
                _ => return Some(Vec::new()),
            };
            return Some(self.on_terminal(message));
        }
        if let Some(picker) = self.terms.picker.as_mut() {
            let last = term::shells().len().saturating_sub(1);
            let message = match chord.key.as_str() {
                "ArrowDown" => {
                    picker.selected = (picker.selected + 1).min(last);
                    return Some(Vec::new());
                }
                "ArrowUp" => {
                    picker.selected = picker.selected.saturating_sub(1);
                    return Some(Vec::new());
                }
                "Enter" => TermMessage::Pick(picker.selected),
                "Escape" => TermMessage::CancelPick,
                digit => match digit.parse::<usize>() {
                    Ok(number @ 1..=9) if plain => TermMessage::Pick(number - 1),
                    _ => return Some(Vec::new()),
                },
            };
            return Some(self.on_terminal(message));
        }
        let view = self.pane(self.active).state.view;
        let asks = matches!(
            self.keymap.lookup(chord, view),
            Some(Command::OpenTerminal { shell: None, .. })
        );
        if asks && term::shells().len() > 1 {
            return Some(self.on_terminal(TermMessage::Choose(self.active)));
        }
        None
    }

    /// A session changed: an ended one is closed, its exit code in its panel's status line; a
    /// closed one loses its widget.
    pub(super) fn terminal_changed(
        &mut self,
        terminal: TermId,
        change: TerminalChange,
    ) -> Vec<Effect> {
        match change {
            TerminalChange::Exited => {
                self.forget(terminal);
                let ended = PanelId::BOTH.into_iter().find_map(|panel| {
                    self.engine
                        .panel_terminals(panel)
                        .into_iter()
                        .find(|info| info.terminal == terminal)
                        .map(|info| (panel, info.state))
                });
                let Some((panel, state)) = ended else {
                    return Vec::new();
                };
                self.pane_mut(panel).error = Some(match state {
                    SessionState::Exited { code: Some(code) } => {
                        t!("terminal.exited", code = code)
                    }
                    _ => t!("terminal.ended").to_owned(),
                });
                vec![Effect::Dispatch(
                    panel,
                    vec![Command::CloseTerminal { terminal }],
                )]
            }
            TerminalChange::Closed => {
                self.forget(terminal);
                Vec::new()
            }
            TerminalChange::Follow => {
                self.follow_notice(terminal);
                Vec::new()
            }
            TerminalChange::LoginScript => {
                self.login_script_changed(terminal);
                Vec::new()
            }
            TerminalChange::Opened
            | TerminalChange::Title
            | TerminalChange::Resized
            | TerminalChange::Folder => Vec::new(),
        }
    }

    /// Why the shell a panel shows is not in the panel's folder, in that panel's status line;
    /// the line clears once the shell got there (RC-040-04).
    fn follow_notice(&mut self, terminal: TermId) {
        let Some(panel) = PanelId::BOTH
            .into_iter()
            .find(|panel| self.engine.shown_terminal(*panel) == Some(terminal))
        else {
            return;
        };
        let status = self
            .engine
            .terminal_session(terminal)
            .and_then(|session| session.follow_status());
        let notice = status.map(|status| match status {
            FollowStatus::NoIntegration => t!("terminal.follow.no_integration"),
            FollowStatus::Running => t!("terminal.follow.running"),
            FollowStatus::LineNotEmpty => t!("terminal.follow.line_not_empty"),
            FollowStatus::Unreachable => t!("terminal.follow.unreachable"),
            FollowStatus::OtherSystem => t!("terminal.follow.other_system"),
        });
        let pane = self.pane_mut(panel);
        match notice {
            Some(notice) => pane.error = Some(notice.to_owned()),
            None if pane
                .follow_notice
                .is_some_and(|shown| pane.error.as_deref() == Some(shown)) =>
            {
                pane.error = None;
            }
            None => {}
        }
        pane.follow_notice = notice;
    }

    /// A login script began a step or ended: the status line says so, and a run that did not
    /// get through says it in the terminal too.
    fn login_script_changed(&mut self, terminal: TermId) {
        let run = PanelId::BOTH
            .into_iter()
            .flat_map(|panel| self.engine.panel_terminals(panel))
            .find(|info| info.terminal == terminal)
            .and_then(|info| info.login_script);
        let Some(run) = run else {
            return;
        };
        let name = run.name.as_str();
        let text = match &run.state {
            LoginRunState::Running { step } => t!(
                "terminal.login.running",
                name = name,
                step = step + 1,
                steps = run.steps
            ),
            LoginRunState::Completed => t!("terminal.login.completed", name = name),
            LoginRunState::TimedOut { step } => {
                t!("terminal.login.timed_out", name = name, step = step + 1)
            }
            LoginRunState::Cancelled => t!("terminal.login.cancelled", name = name),
            LoginRunState::StreamClosed { step } => {
                t!("terminal.login.closed", name = name, step = step + 1)
            }
            LoginRunState::Failed { code } => {
                t!(
                    "terminal.login.failed",
                    name = name,
                    error = error_text(code)
                )
            }
        };
        if run.state.is_finished()
            && run.state != LoginRunState::Completed
            && let Some(session) = self.engine.terminal_session(terminal)
        {
            // Bold red on a line of its own; the shell's prompt follows on the next.
            session.show_local(&format!("\r\n\x1b[1;31m{text}\x1b[0m\r\n"));
        }
        self.info = Some(text);
    }

    /// Drops the widget of a session that is gone, and a paste waiting for it.
    fn forget(&mut self, terminal: TermId) {
        self.terms.widgets.remove(&terminal);
        if self
            .terms
            .paste
            .as_ref()
            .is_some_and(|(waiting, _)| *waiting == terminal)
        {
            self.terms.paste = None;
        }
    }

    /// Makes the widget of the session `panel` shows, then gives it the keyboard.
    pub(super) fn show_terminal(&mut self, panel: PanelId) -> Task<Message> {
        let Some(terminal) = self.shown_terminal(panel) else {
            return Task::none();
        };
        if !self.terms.widgets.contains_key(&terminal) {
            let Some(session) = self.engine.terminal_session(terminal) else {
                return Task::none();
            };
            let reserved = term::reserved_keys(&self.keymap);
            let settings = term::settings(theme::tokens(self.dark), reserved, self.terms.copy);
            self.terms
                .widgets
                .insert(terminal, Terminal::new(session, &settings));
        }
        self.focus_terminal(panel)
    }

    /// Gives the keyboard to `panel`'s terminal when it is the active panel, shows one and no
    /// dialog is open - nor a popup of the tabs, such as the list of sessions (Ctrl+Shift+A).
    pub(super) fn focus_terminal(&self, panel: PanelId) -> Task<Message> {
        if panel != self.active || self.terms.dialog_open() || self.tabs.popup.is_some() {
            return Task::none();
        }
        match self
            .shown_terminal(panel)
            .and_then(|terminal| self.terms.widgets.get(&terminal))
        {
            Some(widget) => TerminalView::focus(widget.widget_id().clone()),
            None => Task::none(),
        }
    }

    /// The terminals in the light or dark look the window just took.
    pub(super) fn restyle_terminals(&mut self) {
        let palette = theme::terminal_palette(theme::tokens(self.dark));
        for widget in self.terms.widgets.values_mut() {
            widget.set_palette(&palette);
        }
    }

    /// The terminals in the font just chosen (RC-010-17); each fits its grid to it.
    pub(super) fn refont_terminals(&mut self) {
        let font = term::font();
        for widget in self.terms.widgets.values_mut() {
            widget.set_font(&font);
        }
    }
}

#[cfg(test)]
#[path = "terminal_pane_tests.rs"]
mod tests;
