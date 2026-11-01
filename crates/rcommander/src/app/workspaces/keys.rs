//! The window's keyboard (RC-100-68). It opens with the keyboard in the filter; typing filters
//! the list. In the filter and in the list the arrows, PageUp and PageDown (Home and End in the
//! list) move the cursor, Enter applies the workspace and closes the window, Insert makes a new
//! one from the panels as they are, F2 renames, Delete (in the list) or F8 deletes after a
//! question, Ctrl+Up and Ctrl+Down move it in the order. Tab and Shift+Tab walk the focus ring
//! (`field_walk.rs`): the filter, the list, the buttons, each control of the form; after each step
//! the ring says which stop has the keyboard and the keys follow it. Escape closes the window
//! from every stop; a name being typed or the delete question take it first.

use iced::widget::Id;
use rc_engine::Chord;

use super::{
    Pending, WsMsg,
    view::{FILTER_FIELD, LIST_STOP_ID},
};
use crate::app::{field_walk, workspaces::list::PAGE};

/// Where the window's keys go: the stop of the focus ring that has the keyboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Filter,
    List,
    /// A stop whose keys are its own: a button, a field or a choice of the form.
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
    Run(WsMsg),
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

/// A key; `captured` when a text field or a stop of the ring used it already.
pub fn key(focus: Focus, pending: Option<&Pending>, chord: &Chord, captured: bool) -> KeyAction {
    let plain = chord.is_plain();
    let escape = chord.key == "Escape" && plain;
    match pending {
        // The name field: Enter submits through the field itself.
        Some(Pending::Name { .. }) if escape => return KeyAction::Run(WsMsg::Cancel),
        Some(Pending::Name { .. }) => return KeyAction::Nothing,
        // A key the Yes or No button took is no answer of its own.
        Some(Pending::Delete(_)) if captured => return KeyAction::Nothing,
        Some(Pending::Delete(_)) => {
            return match (chord.key.as_str(), plain) {
                ("Enter" | "y", true) => KeyAction::Run(WsMsg::Confirm(true)),
                ("Escape" | "n", true) => KeyAction::Run(WsMsg::Confirm(false)),
                _ => KeyAction::Nothing,
            };
        }
        None => {}
    }
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
    if escape {
        return KeyAction::Run(WsMsg::Close);
    }
    if focus == Focus::Ring {
        return KeyAction::Nothing;
    }
    let list = focus == Focus::List;
    let message = match (chord.key.as_str(), chord.ctrl, chord.shift, chord.alt) {
        ("ArrowUp", false, false, false) => WsMsg::Step(-1),
        ("ArrowDown", false, false, false) => WsMsg::Step(1),
        ("PageUp", false, false, false) => WsMsg::Step(-PAGE),
        ("PageDown", false, false, false) => WsMsg::Step(PAGE),
        ("Home", false, false, false) if list => WsMsg::Step(isize::MIN),
        ("End", false, false, false) if list => WsMsg::Step(isize::MAX),
        ("ArrowUp", true, false, false) => WsMsg::Move(true),
        ("ArrowDown", true, false, false) => WsMsg::Move(false),
        ("Enter", false, false, false) => WsMsg::Apply,
        ("Insert", false, false, false) => WsMsg::New,
        ("F2", false, false, false) => WsMsg::Rename,
        ("F8", false, false, false) => WsMsg::Delete,
        ("Delete", false, false, false) if list => WsMsg::Delete,
        _ => return KeyAction::Nothing,
    };
    KeyAction::Run(message)
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
