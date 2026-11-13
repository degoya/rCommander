//! How the terminals copy their selection besides Ctrl+Shift+C (owner 2026-10-25): Ctrl+C and a
//! right click while text is selected, letting go of the mouse after selecting, Shift with the
//! mouse and a program's OSC 52; and whether Ctrl+V and a right click paste - the stored
//! `terminal.copy_*`, `terminal.paste_ctrl_v`, `terminal.paste_right_click`,
//! `terminal.osc52_copy`, `terminal.osc52_remote` and `terminal.shift_selects` settings, read at
//! the start and whenever one changes, in every widget.
//!
//! A program's copy (OSC 52, RC-080-65) reaches the clipboard only from a terminal a panel shows
//! (a hidden one's is dropped), from an SSH terminal only with `terminal.osc52_remote`, and at
//! most `rc_term::osc::MAX_CLIPBOARD` of text (the scanner and the widget drop longer ones); the
//! status line says which terminal set it.

use iced::Task;
use rc_db::Db;
use rc_engine::{
    PanelId, TermId,
    settings::{
        COPY_CTRL_C_KEY, COPY_ON_SELECT_KEY, COPY_RIGHT_CLICK_KEY, OSC52_COPY_KEY,
        OSC52_REMOTE_KEY, PASTE_CTRL_V_KEY, PASTE_RIGHT_CLICK_KEY, SHIFT_SELECTS_KEY, stored_bool,
    },
};
use rc_term_widget::CopySettings;

use super::{App, Effect, Message, terminal_pane::TermMessage};
use crate::i18n::t;

/// Whether `key` is one of the copy and paste settings.
pub fn concerns(key: &str) -> bool {
    [
        COPY_CTRL_C_KEY,
        COPY_RIGHT_CLICK_KEY,
        COPY_ON_SELECT_KEY,
        PASTE_CTRL_V_KEY,
        PASTE_RIGHT_CLICK_KEY,
        OSC52_COPY_KEY,
        OSC52_REMOTE_KEY,
        SHIFT_SELECTS_KEY,
    ]
    .contains(&key)
}

/// The stored copy settings.
async fn load(db: Db) -> CopySettings {
    CopySettings {
        ctrl_c: stored_bool(&db, COPY_CTRL_C_KEY).await,
        right_click: stored_bool(&db, COPY_RIGHT_CLICK_KEY).await,
        right_click_pastes: stored_bool(&db, PASTE_RIGHT_CLICK_KEY).await,
        on_select: stored_bool(&db, COPY_ON_SELECT_KEY).await,
        ctrl_v: stored_bool(&db, PASTE_CTRL_V_KEY).await,
        osc52: stored_bool(&db, OSC52_COPY_KEY).await,
        osc52_remote: stored_bool(&db, OSC52_REMOTE_KEY).await,
        shift_selects: stored_bool(&db, SHIFT_SELECTS_KEY).await,
    }
}

impl App {
    /// Reads the copy settings for the terminals.
    pub(in crate::app) fn terminal_copy_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(load(db), |copy| Message::Terminal(TermMessage::Copy(copy)))
    }

    /// A program in `terminal` put `text` on the clipboard (OSC 52): written when
    /// [`copy_allowed`], with a word in the status line.
    pub(super) fn program_copy(&mut self, terminal: TermId, text: String) -> Vec<Effect> {
        let shown = match self.maximized() {
            Some(panel) => self.shown_terminal(panel) == Some(terminal),
            None => PanelId::BOTH
                .into_iter()
                .any(|panel| self.shown_terminal(panel) == Some(terminal)),
        };
        let info = PanelId::BOTH
            .into_iter()
            .flat_map(|panel| self.engine.panel_terminals(panel))
            .find(|info| info.terminal == terminal);
        let Some(info) = info else {
            return Vec::new();
        };
        let remote = info.connection_id.is_some();
        if !copy_allowed(self.terms.copy, text.len(), shown, remote) {
            tracing::debug!(%terminal, shown, remote, "a program's copy to the clipboard was dropped");
            return Vec::new();
        }
        self.info = Some(t!("terminal.osc52.copied", title = info.title.as_str()));
        vec![Effect::Task(iced::clipboard::write(text))]
    }

    /// The copy settings for the open terminals and the ones still to come.
    pub(super) fn set_terminal_copy(&mut self, copy: CopySettings) {
        self.terms.copy = copy;
        for widget in self.terms.widgets.values_mut() {
            widget.set_copy(copy);
        }
    }
}

/// Whether a program's copy of `len` bytes reaches the clipboard: `terminal.osc52_copy` is on,
/// the text within the limit, a panel shows the terminal (`shown`), and an SSH terminal's
/// (`remote`) only with `terminal.osc52_remote`.
pub(super) fn copy_allowed(copy: CopySettings, len: usize, shown: bool, remote: bool) -> bool {
    copy.osc52 && len <= rc_term::osc::MAX_CLIPBOARD && shown && (!remote || copy.osc52_remote)
}

#[cfg(test)]
#[path = "terminal_copy_tests.rs"]
mod tests;
