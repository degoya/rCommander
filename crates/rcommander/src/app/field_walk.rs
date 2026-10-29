//! Tab and Shift+Tab through the focus ring of the dialog on top (RC-080-16): its text fields and
//! the stops of `theme::focus` - buttons, checkboxes, pick lists, option groups, lists - in
//! reading order, round the end, as in a native Windows dialog. iced moves the keyboard between
//! them only when told to. A dialog marks its card with `theme::DIALOG_ID`; the walk goes round
//! the stops of the card marked last in the window's tree - the top layer, or a question inside
//! a dialog - so it never reaches the panels or a dialog beneath. A text field keeps iced's own
//! focus; a stop draws the ring. A dialog that drives its own keys by where the keyboard is (the
//! connection manager) asks after a walk which stop has it ([`focused`]). The host-key and
//! certificate questions keep their own walk over their buttons; multi-rename keeps its own Tab.

use iced::{
    Rectangle, Task,
    advanced::widget::{
        Id, Operation, operate,
        operation::{Focusable, Outcome},
    },
};
use rc_engine::Chord;

use super::{Effect, Message};
use crate::theme::layout::Cards;

/// Tab (`Some(false)`) or Shift+Tab (`Some(true)`); `None` for any other key.
pub fn tab(chord: &Chord) -> Option<bool> {
    (chord.key == "Tab" && !chord.ctrl && !chord.alt).then_some(chord.shift)
}

/// Whether `chord` is a key a stop of the focus ring takes - Enter, Space, an arrow. Captured
/// (`Message::KeyCaptured`), a stop or a field used it: the dialog must not act on it again.
pub fn taken(chord: &Chord) -> bool {
    matches!(
        chord.key.as_str(),
        "Enter" | "Space" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
    )
}

/// Tab or Shift+Tab as the effect that walks the focus ring; `None` for any other key.
pub fn walk_key(chord: &Chord) -> Option<Vec<Effect>> {
    tab(chord).map(|back| vec![Effect::WalkFields { back, then: None }])
}

/// The task behind [`Effect::WalkFields`].
pub fn walk(back: bool) -> Task<Message> {
    operate(Survey {
        back,
        cards: Cards::default(),
        fields: Vec::new(),
        focused: None,
    })
    .discard()
}

/// The id of the stop or field of the top card that has the keyboard; `None` for one without an
/// id or when none has it.
pub fn focused() -> Task<Option<Id>> {
    operate(Focused {
        cards: Cards::default(),
        found: None,
    })
}

/// The field after (`back`: before) `focused` among `total`, round the end; the first (last)
/// when none has the keyboard.
pub fn next(total: usize, focused: Option<usize>, back: bool) -> Option<usize> {
    if total == 0 {
        return None;
    }
    Some(match (focused, back) {
        (Some(at), false) => (at + 1) % total,
        (Some(at), true) => (at + total - 1) % total,
        (None, false) => 0,
        (None, true) => total - 1,
    })
}

/// The first pass: how many text fields each marked card has and which one has the keyboard.
struct Survey {
    back: bool,
    cards: Cards,
    /// The fields of card `n` at `n - 1`.
    fields: Vec<usize>,
    /// The field with the keyboard: its card and its place in it.
    focused: Option<(usize, usize)>,
}

impl Operation for Survey {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let (outer, card) = self.cards.enter();
        if card {
            self.fields.push(0);
        }
        operate(self);
        self.cards.inside = outer;
    }

    fn container(&mut self, id: Option<&Id>, _bounds: Rectangle) {
        self.cards.container(id);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        let Some(card) = self.cards.inside else {
            return;
        };
        let Some(count) = self.fields.get_mut(card - 1) else {
            return;
        };
        if state.is_focused() {
            self.focused = Some((card, *count));
        }
        *count += 1;
    }

    fn finish(&self) -> Outcome<()> {
        let card = self.fields.len();
        let Some(&total) = self.fields.last() else {
            return Outcome::None;
        };
        let focused = self
            .focused
            .filter(|(at, _)| *at == card)
            .map(|(_, field)| field);
        match next(total, focused, self.back) {
            Some(to) => Outcome::Chain(Box::new(Move {
                cards: Cards::default(),
                card,
                to,
                seen: 0,
            })),
            None => Outcome::None,
        }
    }
}

/// The second pass: the keyboard to field `to` of card `card`, away from every other field.
struct Move {
    cards: Cards,
    card: usize,
    to: usize,
    seen: usize,
}

impl Operation for Move {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let (outer, _) = self.cards.enter();
        operate(self);
        self.cards.inside = outer;
    }

    fn container(&mut self, id: Option<&Id>, _bounds: Rectangle) {
        self.cards.container(id);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if self.cards.inside == Some(self.card) {
            if self.seen == self.to {
                state.focus();
            } else {
                state.unfocus();
            }
            self.seen += 1;
        } else if state.is_focused() {
            state.unfocus();
        }
    }
}

/// The search behind [`focused`]: the focused stop of each card, kept for the last card.
struct Focused {
    cards: Cards,
    /// The card of the stop with the keyboard, and its id.
    found: Option<(usize, Option<Id>)>,
}

impl Operation<Option<Id>> for Focused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<Id>>)) {
        let (outer, _) = self.cards.enter();
        operate(self);
        self.cards.inside = outer;
    }

    fn container(&mut self, id: Option<&Id>, _bounds: Rectangle) {
        self.cards.container(id);
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if let Some(card) = self.cards.inside
            && state.is_focused()
        {
            self.found = Some((card, id.cloned()));
        }
    }

    fn finish(&self) -> Outcome<Option<Id>> {
        let top = self.cards.entered;
        Outcome::Some(
            self.found
                .clone()
                .filter(|(card, _)| *card == top)
                .and_then(|(_, id)| id),
        )
    }
}

#[cfg(test)]
#[path = "field_walk_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "focus_probe.rs"]
mod probe;

#[cfg(test)]
#[path = "focus_ring_tests.rs"]
mod ring_tests;

#[cfg(test)]
#[path = "focus_ring_file_ops_tests.rs"]
mod ring_file_ops_tests;

#[cfg(test)]
#[path = "focus_ring_tools_tests.rs"]
mod ring_tools_tests;

#[cfg(test)]
#[path = "focus_ring_questions_tests.rs"]
mod ring_questions_tests;

#[cfg(test)]
#[path = "focus_ring_settings_tests.rs"]
mod ring_settings_tests;

#[cfg(test)]
#[path = "focus_ring_windows_tests.rs"]
mod ring_windows_tests;

#[cfg(test)]
#[path = "focus_ring_question_windows_tests.rs"]
mod ring_question_windows_tests;

#[cfg(test)]
#[path = "focus_ring_exchange_tests.rs"]
mod ring_exchange_tests;

#[cfg(test)]
#[path = "focus_ring_mnemonics_tests.rs"]
mod ring_mnemonics_tests;

#[cfg(test)]
#[path = "focus_ring_access_tests.rs"]
mod ring_access_tests;
