//! How the terminals copy their selection besides Ctrl+Shift+C (owner 2026-10-25): Ctrl+C and a
//! right click while text is selected, letting go of the mouse after selecting, Shift with the
//! mouse and a program's OSC 52; and whether Ctrl+V pastes - the stored `terminal.copy_*`,
//! `terminal.paste_ctrl_v`, `terminal.osc52_copy` and `terminal.shift_selects` settings, read
//! at the start and whenever one changes, in every widget.

use iced::Task;
use rc_db::Db;
use rc_engine::settings::{
    COPY_CTRL_C_KEY, COPY_ON_SELECT_KEY, COPY_RIGHT_CLICK_KEY, OSC52_COPY_KEY, PASTE_CTRL_V_KEY,
    SHIFT_SELECTS_KEY, stored_bool,
};
use rc_term_widget::CopySettings;

use super::{App, Message, terminal_pane::TermMessage};

/// Whether `key` is one of the copy and paste settings.
pub fn concerns(key: &str) -> bool {
    [
        COPY_CTRL_C_KEY,
        COPY_RIGHT_CLICK_KEY,
        COPY_ON_SELECT_KEY,
        PASTE_CTRL_V_KEY,
        OSC52_COPY_KEY,
        SHIFT_SELECTS_KEY,
    ]
    .contains(&key)
}

/// The stored copy settings.
async fn load(db: Db) -> CopySettings {
    CopySettings {
        ctrl_c: stored_bool(&db, COPY_CTRL_C_KEY).await,
        right_click: stored_bool(&db, COPY_RIGHT_CLICK_KEY).await,
        on_select: stored_bool(&db, COPY_ON_SELECT_KEY).await,
        ctrl_v: stored_bool(&db, PASTE_CTRL_V_KEY).await,
        osc52: stored_bool(&db, OSC52_COPY_KEY).await,
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

    /// The copy settings for the open terminals and the ones still to come.
    pub(super) fn set_terminal_copy(&mut self, copy: CopySettings) {
        self.terms.copy = copy;
        for widget in self.terms.widgets.values_mut() {
            widget.set_copy(copy);
        }
    }
}

#[cfg(test)]
#[path = "terminal_copy_tests.rs"]
mod tests;
