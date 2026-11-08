//! The window's keyboard (RC-100-91), as the own commands' (`user_commands/keys.rs`): it opens with
//! the keyboard in the filter. In the filter and the list the arrows, PageUp and PageDown (Home
//! and End in the list) move the cursor, Ctrl+Up and Ctrl+Down move the rule at the cursor - the
//! order the rules are tried in, as dragging does in Total Commander -, Insert makes a new rule
//! after it, Delete (in the list) or F8 removes it, Enter saves. Tab and Shift+Tab walk the focus
//! ring (`field_walk.rs`); after each step the ring says which stop has the keyboard. Escape
//! closes the window from every stop.

use iced::widget::Id;
use rc_engine::Chord;

use super::{
    AssocMsg,
    form::PAGE,
    view::{FILTER_FIELD, LIST_STOP_ID},
};
use crate::app::field_walk;

/// Where the window's keys go: the stop of the focus ring that has the keyboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Filter,
    List,
    /// A stop whose keys are its own: a field, a choice or a button of the form.
    Ring,
}

/// What a key asks of the window.
#[derive(Debug, Clone)]
pub enum KeyAction {
    Nothing,
    /// Tab (`false`) or Shift+Tab (`true`): the next stop of the ring.
    Walk(bool),
    /// An access key moved the ring: which stop has the keyboard now.
    Follow,
    Run(AssocMsg),
}

/// The keys follow the ring to the stop `id`.
pub fn focus_of(id: Option<&Id>) -> Focus {
    let is = |name: &'static str| id == Some(&Id::new(name));
    if is(FILTER_FIELD) {
        Focus::Filter
    } else if is(LIST_STOP_ID) {
        Focus::List
    } else {
        Focus::Ring
    }
}

/// A key at `focus`; `captured` when a text field or a stop of the ring used it already.
pub fn key(focus: Focus, chord: &Chord, captured: bool) -> KeyAction {
    if captured {
        return if chord.alt && !chord.ctrl {
            KeyAction::Follow
        } else {
            KeyAction::Nothing
        };
    }
    if let Some(back) = field_walk::tab(chord) {
        return KeyAction::Walk(back);
    }
    if chord.key == "Escape" && chord.is_plain() {
        return KeyAction::Run(AssocMsg::Close);
    }
    if focus == Focus::Ring {
        return KeyAction::Nothing;
    }
    let list = focus == Focus::List;
    let message = match (chord.key.as_str(), chord.ctrl, chord.shift, chord.alt) {
        ("ArrowUp", false, false, false) => AssocMsg::Step(-1),
        ("ArrowDown", false, false, false) => AssocMsg::Step(1),
        ("PageUp", false, false, false) => AssocMsg::Step(-PAGE),
        ("PageDown", false, false, false) => AssocMsg::Step(PAGE),
        ("Home", false, false, false) if list => AssocMsg::Step(isize::MIN),
        ("End", false, false, false) if list => AssocMsg::Step(isize::MAX),
        ("ArrowUp", true, false, false) => AssocMsg::Move(true),
        ("ArrowDown", true, false, false) => AssocMsg::Move(false),
        ("Enter", false, false, false) => AssocMsg::Save,
        ("Insert", false, false, false) => AssocMsg::New,
        ("F8", false, false, false) => AssocMsg::Delete,
        ("Delete", false, false, false) if list => AssocMsg::Delete,
        _ => return KeyAction::Nothing,
    };
    KeyAction::Run(message)
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
