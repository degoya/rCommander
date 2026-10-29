//! The small menus of the tabs: the folder history (Alt+Down), the hotlist (Ctrl+D), a tab's
//! own menu (a right click on the tab), the list of all tabs behind `»` (RC-010-21, also
//! Ctrl+Shift+A) and the list of a panel's terminal sessions (Ctrl+Shift+A while the panel shows
//! its terminal, RC-080-16). Pure data, so the keys are tested without a window.
//!
//! As Total Commander's popups: the arrows, Home and End move, Enter or Space takes the row,
//! a letter takes the one row starting with it or moves to the next of several (`menu_letter`),
//! Escape closes. A row without an action (a hotlist folder) is a heading the cursor skips. In
//! the lists of tabs and sessions Delete closes the row's tab or session, and Shift+F10 or the
//! context-menu key open a tab's own menu; in the list of tabs Ctrl+Up and Ctrl+Down move the
//! row's tab one place, as dragging it in the tab bar does (RC-010-24).

use rc_engine::{Chord, HotlistEntry, PanelId, TabId, TabView, TermId, VfsPath, shown_path};

use crate::{
    app::menu_letter::{self, Letter},
    i18n::t,
};

/// What taking a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupAction {
    /// Show entry `index` of the tab's history.
    HistoryJump(usize),
    /// Open a hotlist entry.
    OpenHotlist(i64),
    /// Add the panel's folder to the hotlist.
    AddCurrent,
    /// Open the hotlist dialog.
    Configure,
    /// Show a tab (the list of all tabs).
    SelectTab(TabId),
    LockTab(TabId),
    CopyTab(TabId),
    CloseTab(TabId),
    /// Show a terminal session (the list of sessions).
    ShowTerminal(TermId),
    CloseTerminal(TermId),
}

/// One row of a popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupRow {
    pub label: String,
    /// A second, dimmer text after the label (a tab's full path); empty for none.
    pub detail: String,
    /// The key that runs the same, right-aligned (the tab's menu, RC-010-24).
    pub key: Option<String>,
    /// How far it is indented: the hotlist folders it is in.
    pub depth: usize,
    /// `None` for a heading.
    pub action: Option<PopupAction>,
    /// The folder the tab shows (in the history), the shown tab (in the list of tabs).
    pub current: bool,
}

/// An open popup of a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub panel: PanelId,
    pub rows: Vec<PopupRow>,
    pub cursor: usize,
}

/// What a key does to a popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupKey {
    /// The popup stays (the cursor may have moved).
    Stay,
    Close,
    /// The row's action is taken; the popup closes.
    Take(PopupAction),
    /// The menu of this tab opens in the popup's place.
    TabMenu(TabId),
    /// This tab moved to this place of its row; the popup stays, its rows already moved.
    MoveTab(TabId, usize),
}

fn row(label: String, depth: usize, action: Option<PopupAction>) -> PopupRow {
    PopupRow {
        label,
        detail: String::new(),
        key: None,
        depth,
        action,
        current: false,
    }
}

impl Popup {
    /// The history of `panel`'s shown tab, newest first, the cursor on the folder shown.
    pub fn history(panel: PanelId, entries: &[VfsPath], current: usize) -> Self {
        let rows: Vec<PopupRow> = entries
            .iter()
            .enumerate()
            .rev()
            .map(|(index, path)| PopupRow {
                current: index == current,
                ..row(
                    path.path.display().to_string(),
                    0,
                    Some(PopupAction::HistoryJump(index)),
                )
            })
            .collect();
        let cursor = rows.iter().position(|row| row.current).unwrap_or(0);
        Self {
            panel,
            rows,
            cursor,
        }
    }

    /// The hotlist in menu order, then "add current folder" and "configure".
    pub fn hotlist(panel: PanelId, items: &[HotlistEntry]) -> Self {
        let depths = super::editor::depths(items);
        let mut rows: Vec<PopupRow> = items
            .iter()
            .zip(depths)
            .map(|(item, depth)| {
                let action = (!item.folder).then_some(PopupAction::OpenHotlist(item.id));
                row(item.name.clone(), depth, action)
            })
            .collect();
        if rows.is_empty() {
            rows.push(row(t!("hotlist.empty").to_owned(), 0, None));
        }
        rows.push(row(
            t!("hotlist.add_current").to_owned(),
            0,
            Some(PopupAction::AddCurrent),
        ));
        rows.push(row(
            t!("hotlist.configure").to_owned(),
            0,
            Some(PopupAction::Configure),
        ));
        let mut popup = Self {
            panel,
            rows,
            cursor: 0,
        };
        popup.cursor = popup.next_action(0, 1).unwrap_or(0);
        popup
    }

    /// The menu of tab `tab` of `panel`, each entry with the key `key_of` gives its registry
    /// command (RC-010-24).
    pub fn tab_menu(
        panel: PanelId,
        tab: TabId,
        locked: bool,
        key_of: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let lock = if locked {
            t!("tabs.menu_unlock")
        } else {
            t!("tabs.menu_lock")
        };
        let entries = [
            (lock, "tabs.lock", PopupAction::LockTab(tab)),
            (
                t!("tabs.menu_copy"),
                "tabs.copy_to_other",
                PopupAction::CopyTab(tab),
            ),
            (
                t!("tabs.menu_close"),
                "tabs.close",
                PopupAction::CloseTab(tab),
            ),
        ];
        let rows = entries
            .into_iter()
            .map(|(label, command, action)| PopupRow {
                key: key_of(command),
                ..row(label.to_owned(), 0, Some(action))
            })
            .collect();
        Self {
            panel,
            rows,
            cursor: 0,
        }
    }

    /// Every tab of `panel` in the row's order, a locked one marked, each with its full path;
    /// the cursor on the tab shown.
    pub fn tabs(panel: PanelId, tabs: &[TabView]) -> Self {
        let rows: Vec<PopupRow> = tabs
            .iter()
            .map(|tab| {
                let label = if tab.locked {
                    t!("tabs.locked", title = tab.title)
                } else {
                    tab.title.clone()
                };
                PopupRow {
                    detail: shown_path(&tab.path).display().to_string(),
                    current: tab.active,
                    ..row(label, 0, Some(PopupAction::SelectTab(tab.id)))
                }
            })
            .collect();
        let cursor = rows.iter().position(|row| row.current).unwrap_or(0);
        Self {
            panel,
            rows,
            cursor,
        }
    }

    /// The terminal sessions of `panel` - each its id, its title and whether the panel shows it
    /// - in the bar's order, the cursor on the one shown.
    pub fn sessions(
        panel: PanelId,
        sessions: impl IntoIterator<Item = (TermId, String, bool)>,
    ) -> Self {
        let rows: Vec<PopupRow> = sessions
            .into_iter()
            .map(|(terminal, title, shown)| PopupRow {
                current: shown,
                ..row(title, 0, Some(PopupAction::ShowTerminal(terminal)))
            })
            .collect();
        let cursor = rows.iter().position(|row| row.current).unwrap_or(0);
        Self {
            panel,
            rows,
            cursor,
        }
    }

    /// What `chord` does.
    pub fn key(&mut self, chord: &Chord) -> PopupKey {
        let plain = !chord.ctrl && !chord.alt;
        let last = self.rows.len().saturating_sub(1);
        if menu_letter::is_letter(chord) {
            return self.letter(&chord.key);
        }
        let row_action = self
            .rows
            .get(self.cursor)
            .and_then(|row| row.action.clone());
        let menu_key = chord.key == "ContextMenu" || (chord.key == "F10" && chord.shift);
        if plain && menu_key {
            return match row_action {
                Some(PopupAction::SelectTab(tab)) => PopupKey::TabMenu(tab),
                _ => PopupKey::Stay,
            };
        }
        match chord.key.as_str() {
            "Escape" => PopupKey::Close,
            "Delete" if plain && !chord.shift => match row_action {
                Some(PopupAction::SelectTab(tab)) => PopupKey::Take(PopupAction::CloseTab(tab)),
                Some(PopupAction::ShowTerminal(terminal)) => {
                    PopupKey::Take(PopupAction::CloseTerminal(terminal))
                }
                _ => PopupKey::Stay,
            },
            "ArrowUp" | "ArrowDown" if chord.ctrl && !chord.alt && !chord.shift => {
                self.move_row(chord.key == "ArrowDown")
            }
            // Alt+Down again closes the history, as it opened it.
            "ArrowDown" if chord.alt => PopupKey::Close,
            "ArrowUp" if plain => self.move_to(
                self.cursor
                    .checked_sub(1)
                    .and_then(|from| self.next_action(from, -1)),
            ),
            "ArrowDown" if plain => self.move_to(self.next_action(self.cursor + 1, 1)),
            "Home" | "PageUp" if plain => self.move_to(self.next_action(0, 1)),
            "End" | "PageDown" if plain => self.move_to(self.next_action(last, -1)),
            "Enter" | "Space" if plain => self.take(self.cursor),
            _ => PopupKey::Stay,
        }
    }

    /// A letter: the one row starting with it is taken, of several the next is chosen.
    fn letter(&mut self, key: &str) -> PopupKey {
        let labels = self
            .rows
            .iter()
            .map(|row| row.action.as_ref().map(|_| row.label.as_str()));
        match menu_letter::letter(key, labels, self.cursor) {
            Letter::None => PopupKey::Stay,
            Letter::Move(index) => self.move_to(Some(index)),
            Letter::Take(index) => self.take(index),
        }
    }

    /// A click on row `index`.
    pub fn take(&self, index: usize) -> PopupKey {
        match self.rows.get(index).and_then(|row| row.action.clone()) {
            Some(action) => PopupKey::Take(action),
            None => PopupKey::Stay,
        }
    }

    /// Ctrl+Up or Ctrl+Down on a row of the list of tabs: its tab moves one place, the cursor
    /// with it; nothing at an end or in another popup.
    fn move_row(&mut self, down: bool) -> PopupKey {
        let action = self
            .rows
            .get(self.cursor)
            .and_then(|row| row.action.clone());
        let Some(PopupAction::SelectTab(tab)) = action else {
            return PopupKey::Stay;
        };
        let to = if down {
            self.cursor + 1
        } else {
            match self.cursor.checked_sub(1) {
                Some(to) => to,
                None => return PopupKey::Stay,
            }
        };
        if to >= self.rows.len() {
            return PopupKey::Stay;
        }
        self.rows.swap(self.cursor, to);
        self.cursor = to;
        PopupKey::MoveTab(tab, to)
    }

    fn move_to(&mut self, index: Option<usize>) -> PopupKey {
        if let Some(index) = index {
            self.cursor = index;
        }
        PopupKey::Stay
    }

    /// The first row with an action from `from` on, walking by `step` (1 or -1).
    fn next_action(&self, from: usize, step: isize) -> Option<usize> {
        let mut index = Some(from);
        while let Some(at) = index.filter(|at| *at < self.rows.len()) {
            if self.rows[at].action.is_some() {
                return Some(at);
            }
            index = at.checked_add_signed(step);
        }
        None
    }
}

#[cfg(test)]
#[path = "popup_tests.rs"]
mod tests;
