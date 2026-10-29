//! The queue window's context menu (RC-010-15, owner 2026-10-18): a right click on a job,
//! Shift+F10 or the context-menu key open it over the job list, for the selection. It offers
//! every action on the selected jobs - up, down, to the front, to the end, into another list or a
//! new one, remove -, marks every job, and clears the list or every list; an entry that does not
//! apply to the selection is dimmed - a move for a job run at once too (RC-010-25). As the tabs'
//! popups: the arrows, Home and End move past the dimmed entries, Enter or Space takes one, Escape
//! closes. Pure data, so the keys are tested without a window.

use rc_engine::{Chord, QueueListId, Reorder};

use super::{QueueWindow, lists::list_choices};
use crate::i18n::t;

/// What an entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Move(Reorder),
    /// Into another list.
    Assign(QueueListId),
    /// Into a new list, after its name.
    Gather,
    Remove,
    SelectAll,
    Clear {
        all_lists: bool,
    },
}

/// One entry: its label, its key, what it does and whether it applies now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub key: Option<&'static str>,
    pub action: MenuAction,
    pub enabled: bool,
}

/// The open menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueMenu {
    pub items: Vec<MenuItem>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub enum MenuMsg {
    /// A click on entry `index`.
    Pick(usize),
    /// The pointer on entry `index`.
    Hover(usize),
    Close,
}

/// What a key or a click does to the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    Stay,
    Close,
    /// The entry's action is taken; the menu closes.
    Take(MenuAction),
}

impl QueueMenu {
    /// The menu for the selection of `window`, the cursor on the first entry that applies.
    pub fn new(window: &QueueWindow) -> Self {
        let chosen = window.chosen_entries();
        let any = !chosen.is_empty();
        let waiting = chosen.iter().any(|entry| entry.place.is_some());
        // A job run at once goes into no list (RC-010-25); the engine would refuse the move.
        let movable = any && chosen.iter().all(|entry| !entry.at_once);
        let listed = !window.entries.is_empty();
        let item = |label: String, key, action, enabled| MenuItem {
            label,
            key,
            action,
            enabled,
        };
        let moves = [
            (t!("queue.menu.up"), "Ctrl+Up", Reorder::Up),
            (t!("queue.menu.down"), "Ctrl+Down", Reorder::Down),
            (t!("queue.menu.front"), "Ctrl+Home", Reorder::Front),
            (t!("queue.menu.back"), "Ctrl+End", Reorder::Back),
        ];
        let mut items: Vec<MenuItem> = moves
            .into_iter()
            .map(|(label, key, step)| {
                item(label.to_owned(), Some(key), MenuAction::Move(step), waiting)
            })
            .collect();
        for choice in list_choices(&window.lists) {
            if choice.id != window.list {
                let label = t!("queue.menu.assign", list = choice);
                items.push(item(label, None, MenuAction::Assign(choice.id), movable));
            }
        }
        let rest = [
            (t!("queue.menu.gather"), None, MenuAction::Gather, movable),
            (
                t!("queue.menu.remove"),
                Some("Delete"),
                MenuAction::Remove,
                any,
            ),
            (
                t!("queue.menu.select_all"),
                Some("Ctrl+A"),
                MenuAction::SelectAll,
                listed,
            ),
            (
                t!("queue.menu.clear"),
                None,
                MenuAction::Clear { all_lists: false },
                listed,
            ),
            (
                t!("queue.menu.clear_all"),
                None,
                MenuAction::Clear { all_lists: true },
                true,
            ),
        ];
        items.extend(
            rest.into_iter()
                .map(|(label, key, action, enabled)| item(label.to_owned(), key, action, enabled)),
        );
        let mut menu = Self { items, cursor: 0 };
        menu.cursor = menu.next(0, 1).unwrap_or(0);
        menu
    }

    /// What `chord` does.
    pub fn key(&mut self, chord: &Chord) -> MenuKey {
        let plain = chord.is_plain();
        let last = self.items.len().saturating_sub(1);
        match chord.key.as_str() {
            "Escape" | "ContextMenu" if plain => MenuKey::Close,
            "F10" if chord.shift && !chord.ctrl && !chord.alt => MenuKey::Close,
            "ArrowUp" if plain => self.move_to(
                self.cursor
                    .checked_sub(1)
                    .and_then(|from| self.next(from, -1)),
            ),
            "ArrowDown" if plain => self.move_to(self.next(self.cursor + 1, 1)),
            "Home" | "PageUp" if plain => self.move_to(self.next(0, 1)),
            "End" | "PageDown" if plain => self.move_to(self.next(last, -1)),
            "Enter" | "Space" if plain => self.take(self.cursor),
            _ => MenuKey::Stay,
        }
    }

    /// A click on entry `index`: a dimmed one does nothing.
    pub fn take(&self, index: usize) -> MenuKey {
        match self.items.get(index).filter(|item| item.enabled) {
            Some(item) => MenuKey::Take(item.action),
            None => MenuKey::Stay,
        }
    }

    /// The pointer on entry `index`: the cursor follows it onto an entry that applies.
    pub fn hover(&mut self, index: usize) {
        if self.items.get(index).is_some_and(|item| item.enabled) {
            self.cursor = index;
        }
    }

    fn move_to(&mut self, index: Option<usize>) -> MenuKey {
        if let Some(index) = index {
            self.cursor = index;
        }
        MenuKey::Stay
    }

    /// The first entry that applies from `from` on, walking by `step` (1 or -1).
    fn next(&self, from: usize, step: isize) -> Option<usize> {
        let mut index = Some(from);
        while let Some(at) = index.filter(|at| *at < self.items.len()) {
            if self.items[at].enabled {
                return Some(at);
            }
            index = at.checked_add_signed(step);
        }
        None
    }
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
