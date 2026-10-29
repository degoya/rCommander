//! Where the manager starts (RC-050-15): the keyboard in the list, on the connection used last -
//! the row the cursor was on when the manager closed in this run, else the one the engine logged
//! in to last (`ConnectionTree::last_used`), else the first row; a folder that hides it opens.
//! The setting `connections.focus_list` off puts the keyboard in the filter field instead, as
//! before. In the list, a character typed goes into the filter, so typing still filters at once.

use iced::widget::operation;
use rc_engine::{Chord, Command, Outcome, TreeItem, settings::CONNECTIONS_FOCUS_LIST_KEY};

use super::{
    ConnectionManager,
    jobs::Then,
    keys::{Focus, focus_id},
    run,
    view::{FILTER_FIELD, TREE_SCROLL_ID},
};
use crate::app::{Effect, scroll};

/// Reads whether the manager opens with the keyboard in the list.
pub(super) fn read_focus_setting() -> Effect {
    run(
        Then::FocusSetting,
        Command::GetSettings {
            key: Some(CONNECTIONS_FOCUS_LIST_KEY.to_owned()),
        },
    )
}

impl ConnectionManager {
    /// The setting's answer: off, the keyboard goes to the filter field. A setting that cannot
    /// be read leaves it in the list, the default.
    pub(super) fn focus_setting(&mut self, result: Result<Outcome, &'static str>) -> Vec<Effect> {
        let on = match result {
            Ok(Outcome::Settings { settings }) => settings
                .iter()
                .find(|setting| setting.key == CONNECTIONS_FOCUS_LIST_KEY)
                .and_then(|setting| setting.value.as_bool()),
            _ => None,
        };
        if on == Some(false) && self.focus == Focus::Tree {
            self.focus = Focus::Filter;
            return vec![focus_id(FILTER_FIELD)];
        }
        Vec::new()
    }

    /// The first tree: the cursor on the starting row, scrolled into view. Later trees keep the
    /// cursor where it is.
    pub(super) fn place_start(&mut self) -> Vec<Effect> {
        if self.started {
            return Vec::new();
        }
        self.started = true;
        let last_used = self.tree.tree.last_used.map(TreeItem::Connection);
        let found = [self.start, last_used]
            .into_iter()
            .flatten()
            .find(|item| self.reveal(*item));
        self.tree.cursor = found;
        if found.is_none() {
            self.tree.jump(false);
        }
        self.show(self.tree.cursor);
        let rows = self.tree.rows();
        match self.tree.cursor_index(&rows) {
            Some(at) => vec![scroll::keep_in_view(TREE_SCROLL_ID, at, rows.len())],
            None => Vec::new(),
        }
    }

    /// Opens the folders around `item`; whether it is a row then.
    fn reveal(&mut self, item: TreeItem) -> bool {
        let mut at = item;
        while let Some(parent) = self.tree.parent_of(at) {
            self.tree.fold(parent, Some(true));
            at = TreeItem::Folder(parent);
        }
        self.tree.rows().iter().any(|row| row.item == item)
    }

    /// A character typed in the list: added to the filter, which takes the keyboard with the
    /// text cursor at its end; `None` for a key that is no character (or Space, the drive-bar
    /// mark). With Shift a letter is a capital; another character with Shift depends on the
    /// keyboard layout and is left alone.
    pub(super) fn type_into_filter(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        let mut chars = chord.key.chars();
        let typed = match (chars.next(), chars.next()) {
            (Some(c), None) if !c.is_whitespace() && !c.is_control() => c,
            _ => return None,
        };
        if chord.ctrl || chord.alt || (chord.shift && !typed.is_alphabetic()) {
            return None;
        }
        if chord.shift {
            self.tree.filter.extend(typed.to_uppercase());
        } else {
            self.tree.filter.push(typed);
        }
        self.focus = Focus::Filter;
        Some(vec![
            focus_id(FILTER_FIELD),
            Effect::Task(operation::move_cursor_to_end(iced::widget::Id::new(
                FILTER_FIELD,
            ))),
        ])
    }
}

#[cfg(test)]
#[path = "start_tests.rs"]
mod tests;
