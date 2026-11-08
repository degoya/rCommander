//! The keyboard in a bar (RC-100-87), as Total Commander's after Alt+Shift+F11 or Alt+Shift+F12:
//! the arrows along the bar (Left and Right above the panels, Up and Down between them), Home and
//! End, Enter runs the button at the cursor, the arrow across the bar (Down, Right between the
//! panels) or Space opens its sub-bar - or runs it -, Shift+F10 and the context-menu key the
//! button's menu, Escape and Tab back to the panel. Separators are passed over. Every other key
//! stays in the bar: a panel's key does nothing while a bar has the keyboard.

use rc_engine::{BarItem, BarPlace, Chord};

use super::{App, BarFocus, Effect};

/// What a key does in a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKey {
    /// The cursor to the item at this place.
    Move(usize),
    /// The button at the cursor: run it, or open its sub-bar.
    Press,
    /// Its sub-bar as a menu; a command's button runs.
    Open,
    /// The button's menu.
    Context,
    /// The keyboard back to the panel.
    Leave,
    /// Nothing.
    Stay,
}

/// The first button of `items`, past separators.
pub fn first(items: &[BarItem]) -> Option<usize> {
    items.iter().position(|item| item.button().is_some())
}

/// The last button of `items`.
fn last(items: &[BarItem]) -> Option<usize> {
    items.iter().rposition(|item| item.button().is_some())
}

/// The button after (`forward`) or before `at`, past separators; `at` itself at the end.
fn step(items: &[BarItem], at: usize, forward: bool) -> usize {
    let found = if forward {
        items
            .iter()
            .enumerate()
            .skip(at + 1)
            .find(|(_, item)| item.button().is_some())
    } else {
        items
            .iter()
            .enumerate()
            .take(at)
            .rev()
            .find(|(_, item)| item.button().is_some())
    };
    found.map_or(at, |(index, _)| index)
}

/// What `chord` does in the bar at `place` whose items are `items`, the cursor at `at`.
pub fn key(chord: &Chord, place: BarPlace, items: &[BarItem], at: usize) -> BarKey {
    let plain = !chord.ctrl && !chord.alt && !chord.shift;
    let (back, forward, across) = match place {
        BarPlace::Horizontal => ("ArrowLeft", "ArrowRight", "ArrowDown"),
        BarPlace::Vertical => ("ArrowUp", "ArrowDown", "ArrowRight"),
    };
    let key = chord.key.as_str();
    match key {
        "F10" if chord.shift && !chord.ctrl && !chord.alt => BarKey::Context,
        "ContextMenu" => BarKey::Context,
        "Escape" | "Tab" => BarKey::Leave,
        _ if !plain => BarKey::Stay,
        _ if key == back => BarKey::Move(step(items, at, false)),
        _ if key == forward => BarKey::Move(step(items, at, true)),
        "Home" => first(items).map_or(BarKey::Stay, BarKey::Move),
        "End" => last(items).map_or(BarKey::Stay, BarKey::Move),
        "Enter" => BarKey::Press,
        "Space" => BarKey::Open,
        _ if key == across => BarKey::Open,
        _ => BarKey::Stay,
    }
}

impl App {
    /// A key while the bar at `focus` has the keyboard.
    pub(super) fn bar_key(&mut self, focus: BarFocus, chord: &Chord) -> Vec<Effect> {
        let Some(bar) = self.button_bar.shown(focus.place) else {
            self.button_bar.focus = None;
            return Vec::new();
        };
        let items = &bar.spec.items;
        match key(chord, focus.place, items, focus.index) {
            BarKey::Move(index) => {
                self.button_bar.focus = Some(BarFocus { index, ..focus });
                Vec::new()
            }
            // A sub-bar opens, a command runs, either way.
            BarKey::Press | BarKey::Open => self.press_bar_item(focus.place, focus.index),
            BarKey::Context => self.open_button_menu(focus.place, focus.index),
            BarKey::Leave => {
                self.button_bar.focus = None;
                Vec::new()
            }
            BarKey::Stay => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
