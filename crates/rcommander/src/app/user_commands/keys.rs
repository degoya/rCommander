//! The window's keyboard (RC-100-86). It opens with the keyboard in the own commands' filter, or
//! on the Start menu's tree; Ctrl+Tab and Ctrl+Shift+Tab turn the tabs. In the filter, the list
//! and the tree the arrows, PageUp and PageDown (Home and End in the list) move the cursor,
//! Insert makes a new command or row, Delete (in the list) or F8 deletes it after a question,
//! Enter saves the form; in the tree Ctrl+Up and Ctrl+Down move the row among its neighbours,
//! Ctrl+Right into the submenu above it and Ctrl+Left out of its own. Tab and Shift+Tab walk the
//! focus ring (`field_walk.rs`); after each step the ring says which stop has the keyboard and
//! the keys follow it. Escape closes the window from every stop; the delete question takes it
//! first.

use iced::widget::Id;
use rc_engine::Chord;

use super::{
    Pending, Tab, UcMsg,
    list::{Move, PAGE},
    view::{FILTER_FIELD, LIST_STOP_ID, TREE_STOP_ID},
};
use crate::app::field_walk;

/// Where the window's keys go: the stop of the focus ring that has the keyboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Filter,
    /// The own commands' list or the Start menu's tree.
    List,
    /// A stop whose keys are its own: a button, a field or a choice of the form.
    Ring,
}

impl Focus {
    /// Where the keyboard starts on `tab`: the filter of the own commands, the Start menu's tree.
    pub fn first(tab: Tab) -> Self {
        match tab {
            Tab::Own => Self::Filter,
            Tab::Start => Self::List,
        }
    }
}

/// What a key asks of the window.
#[derive(Debug, Clone)]
pub enum KeyAction {
    Nothing,
    /// Tab (`false`) or Shift+Tab (`true`): the next stop of the ring.
    Walk(bool),
    /// An access key moved the ring: which stop has the keyboard now.
    Follow,
    Run(UcMsg),
}

/// The keys follow the ring to the stop `id`.
pub fn focus_of(id: Option<&Id>) -> Focus {
    let is = |name: &'static str| id == Some(&Id::new(name));
    if is(FILTER_FIELD) {
        Focus::Filter
    } else if is(LIST_STOP_ID) || is(TREE_STOP_ID) {
        Focus::List
    } else {
        Focus::Ring
    }
}

/// A key on `tab`; `captured` when a text field or a stop of the ring used it already.
pub fn key(
    (tab, focus): (Tab, Focus),
    pending: Option<&Pending>,
    chord: &Chord,
    captured: bool,
) -> KeyAction {
    let plain = chord.is_plain();
    if pending.is_some() {
        // A key the Yes or No button took is no answer of its own.
        if captured {
            return KeyAction::Nothing;
        }
        return match (chord.key.as_str(), plain) {
            ("Enter" | "y", true) => KeyAction::Run(UcMsg::Confirm(true)),
            ("Escape" | "n", true) => KeyAction::Run(UcMsg::Confirm(false)),
            _ => KeyAction::Nothing,
        };
    }
    if chord.key == "Tab" && chord.ctrl && !chord.alt {
        // Two tabs: Ctrl+Tab and Ctrl+Shift+Tab both turn to the other.
        let other = match tab {
            Tab::Own => Tab::Start,
            Tab::Start => Tab::Own,
        };
        return KeyAction::Run(UcMsg::Tab(other));
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
    if chord.key == "Escape" && plain {
        return KeyAction::Run(UcMsg::Close);
    }
    if focus == Focus::Ring {
        return KeyAction::Nothing;
    }
    let list = focus == Focus::List;
    let tree = tab == Tab::Start;
    let message = match (chord.key.as_str(), chord.ctrl, chord.shift, chord.alt) {
        ("ArrowUp", false, false, false) => UcMsg::Step(-1),
        ("ArrowDown", false, false, false) => UcMsg::Step(1),
        ("PageUp", false, false, false) => UcMsg::Step(-PAGE),
        ("PageDown", false, false, false) => UcMsg::Step(PAGE),
        ("Home", false, false, false) if list => UcMsg::Step(isize::MIN),
        ("End", false, false, false) if list => UcMsg::Step(isize::MAX),
        ("ArrowUp", true, false, false) if tree => UcMsg::Move(Move::Up),
        ("ArrowDown", true, false, false) if tree => UcMsg::Move(Move::Down),
        ("ArrowRight", true, false, false) if tree => UcMsg::Move(Move::Into),
        ("ArrowLeft", true, false, false) if tree => UcMsg::Move(Move::Out),
        ("Enter", false, false, false) => UcMsg::Save,
        ("Insert", false, false, false) => UcMsg::New,
        ("F8", false, false, false) => UcMsg::Delete,
        ("Delete", false, false, false) if list => UcMsg::Delete,
        _ => return KeyAction::Nothing,
    };
    KeyAction::Run(message)
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
