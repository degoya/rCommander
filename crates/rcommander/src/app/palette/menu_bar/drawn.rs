//! The menu bar rCommander draws itself where no system menu is attached - Linux and, until its
//! own menu comes, macOS (RC-080-33): the menus of [`super::build`] as a row of titles above the
//! drive bar, an open menu as a card under its title (`view.rs`). A choice is the same
//! [`MenuBarMsg::Chosen`] the Win32 menu sends, so it runs what the palette runs.
//!
//! It follows the setting `appearance.menu_bar` and is greyed out while a dialog is open, as the
//! Win32 bar is. The keyboard is a Win32 menu's: Alt with a title's letter opens its menu (unless
//! the quick search or a terminal wants Alt with letters); open, the arrows walk the entries and
//! the menus, Enter or an entry's letter takes it, Escape closes. Alt alone does nothing - no
//! chord names it.

use rc_engine::Chord;

use super::{MenuBarMsg, build, reach};
use crate::{
    app::{App, Effect},
    platform::menu_bar::{BarItem, BarMenu},
    theme::mnemonic::Label,
};

/// Whether rCommander draws the bar: everywhere but Windows, which attaches a Win32 menu.
pub const DRAWN: bool = cfg!(not(windows));

/// The open menu: its index among the menus, and the entry the keyboard or the pointer is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenMenu {
    pub menu: usize,
    pub cursor: Option<usize>,
}

/// What a key does in an open menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Stay,
    Close,
    Choose(&'static str),
}

/// The title's or the entry's access key, in lower case.
fn letter(text: &str) -> Option<char> {
    Label::parse(text.split('\t').next().unwrap_or_default()).letter
}

/// The key the chord types for an access key: one character in lower case.
fn typed(chord: &Chord) -> Option<char> {
    let mut chars = chord.key.chars();
    let first = chars.next()?;
    chars
        .next()
        .is_none()
        .then(|| first.to_lowercase().next())
        .flatten()
}

/// The menu whose title has the letter `chord` types with Alt alone.
pub fn menu_of(menus: &[BarMenu], chord: &Chord) -> Option<usize> {
    if !chord.alt || chord.ctrl || chord.shift {
        return None;
    }
    let key = typed(chord)?;
    menus
        .iter()
        .position(|menu| letter(&menu.title) == Some(key))
}

/// The entry before or after `from` (`None`: from outside the menu), skipping separators and
/// wrapping round.
fn walk(items: &[BarItem], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = items.len();
    let start = match (from, forward) {
        (Some(at), _) => at,
        (None, true) => count.checked_sub(1)?,
        (None, false) => 0,
    };
    (1..=count)
        .map(|step| {
            if forward {
                (start + step) % count
            } else {
                (start + count - step % count) % count
            }
        })
        .find(|&index| matches!(items[index], BarItem::Entry { .. }))
}

/// The first entry of `menu`.
pub fn first(menus: &[BarMenu], menu: usize) -> Option<usize> {
    walk(&menus.get(menu)?.items, None, true)
}

/// What `chord` does in the open menu `open`, which it may move.
pub fn key(menus: &[BarMenu], open: &mut OpenMenu, chord: &Chord) -> Step {
    let Some(items) = menus.get(open.menu).map(|menu| menu.items.as_slice()) else {
        return Step::Close;
    };
    if let Some(menu) = menu_of(menus, chord) {
        *open = OpenMenu {
            menu,
            cursor: first(menus, menu),
        };
        return Step::Stay;
    }
    let plain = chord.is_plain();
    if plain && let Some(key) = typed(chord) {
        return items
            .iter()
            .find_map(|item| match item {
                BarItem::Entry { id, text } if letter(text) == Some(key) => Some(Step::Choose(id)),
                _ => None,
            })
            .unwrap_or(Step::Stay);
    }
    let count = menus.len();
    let to_menu = |menu: usize| OpenMenu {
        menu,
        cursor: first(menus, menu),
    };
    match chord.key.as_str() {
        "Escape" => return Step::Close,
        "ArrowLeft" if plain => *open = to_menu((open.menu + count - 1) % count),
        "ArrowRight" if plain => *open = to_menu((open.menu + 1) % count),
        "ArrowUp" if plain => open.cursor = walk(items, open.cursor, false),
        "ArrowDown" if plain => open.cursor = walk(items, open.cursor, true),
        "Home" | "PageUp" if plain => open.cursor = walk(items, None, true),
        "End" | "PageDown" if plain => open.cursor = walk(items, None, false),
        "Enter" | "Space" if plain => {
            return match open.cursor.and_then(|at| items.get(at)) {
                Some(BarItem::Entry { id, .. }) => Step::Choose(id),
                _ => Step::Close,
            };
        }
        _ => {}
    }
    Step::Stay
}

impl App {
    /// The menus again, with the keys of the keymap in force: when the bar is shown and after the
    /// keymap changed.
    pub(in crate::app) fn rebuild_drawn_menus(&mut self) {
        let entries =
            super::super::entries(&self.keymap, &reach::standard(), self.drive_bar.network);
        self.dialog_windows.menu_bar.menus = build(&self.keymap, &entries);
    }

    /// Whether the drawn bar shows.
    pub(in crate::app) fn drawn_bar_shown(&self) -> bool {
        DRAWN && self.dialog_windows.menu_bar.shown == Some(true)
    }

    /// Whether the drawn bar takes `chord`: while a menu is open every key, else Alt with a
    /// title's letter, when no dialog is open and Alt with letters is not wanted elsewhere.
    pub(in crate::app) fn drawn_bar_has_key(&self, chord: &Chord) -> bool {
        let bar = &self.dialog_windows.menu_bar;
        bar.open.is_some()
            || (self.drawn_bar_shown()
                && !self.dialog_open()
                && self.alt_letters_free()
                && menu_of(&bar.menus, chord).is_some())
    }

    pub(in crate::app) fn drawn_bar_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let bar = &mut self.dialog_windows.menu_bar;
        let mut open = match bar.open {
            Some(open) => open,
            None => match menu_of(&bar.menus, chord) {
                Some(menu) => {
                    bar.open = Some(OpenMenu {
                        menu,
                        cursor: first(&bar.menus, menu),
                    });
                    return Vec::new();
                }
                None => return Vec::new(),
            },
        };
        match key(&bar.menus, &mut open, chord) {
            Step::Stay => {
                bar.open = Some(open);
                Vec::new()
            }
            Step::Close => {
                bar.open = None;
                Vec::new()
            }
            Step::Choose(id) => self.on_menu_bar(MenuBarMsg::Chosen(id.to_owned())),
        }
    }

    /// A click on the title of `menu`: opens it, or closes it when it is open. Nothing while a
    /// dialog is open.
    pub(super) fn on_drawn_title(&mut self, menu: usize) {
        let bar = &self.dialog_windows.menu_bar;
        if bar.open.is_none() && self.dialog_open() {
            return;
        }
        let open = (bar.open.map(|open| open.menu) != Some(menu))
            .then_some(OpenMenu { menu, cursor: None });
        self.dialog_windows.menu_bar.open = open;
    }
}

#[cfg(test)]
#[path = "drawn_tests.rs"]
mod tests;
