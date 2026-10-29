//! The tabs of both panels as engine state: pure data, changed under the engine's state lock.
//!
//! The panel's [`PanelState`] is the active tab's live state; the tab keeps its own copy of
//! folder, sort and mode only while another tab is shown, and [`Tabs::sync`] brings it up to date
//! before the panel moves to another tab. So every command that works on a panel works on the
//! active tab without knowing tabs exist.

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use super::{history::History, title::titles};
use crate::{ListMode, PanelId, PanelState, SortOrder};

/// A tab's id: unique among both panels' tabs while the engine runs, never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TabId(pub u64);

/// One tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    /// Its folder; the panel's own while the tab is active.
    pub path: VfsPath,
    pub sort: SortOrder,
    pub mode: ListMode,
    /// A locked tab keeps its folder: a new folder opens in a new tab.
    pub locked: bool,
    pub history: History,
    /// The entry the cursor stood on when the tab was left.
    pub cursor: Option<String>,
}

/// A tab as a command reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabView {
    pub id: TabId,
    /// The folder's last part; told apart by its parent, or a number, where two are equal.
    pub title: String,
    pub path: VfsPath,
    pub locked: bool,
    /// Whether the panel shows this tab.
    pub active: bool,
}

/// One panel's tabs, never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSet {
    tabs: Vec<Tab>,
    active: usize,
}

impl TabSet {
    /// The tabs, left to right.
    #[must_use]
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tabs_mut(&mut self) -> &mut [Tab] {
        &mut self.tabs
    }

    /// Index of the tab the panel shows.
    #[must_use]
    pub fn active(&self) -> usize {
        self.active
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// Index of the tab `id`.
    #[must_use]
    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// A set read back; `None` for no tabs. The active index is clamped.
    #[must_use]
    pub fn from_tabs(tabs: Vec<Tab>, active: usize) -> Option<Self> {
        if tabs.is_empty() {
            return None;
        }
        let active = active.min(tabs.len() - 1);
        Some(Self { tabs, active })
    }
}

/// The tabs of both panels and the next free id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tabs {
    sets: [TabSet; 2],
    next_id: u64,
}

impl Tabs {
    /// One tab per panel, on the panels' folders.
    #[must_use]
    pub fn new(left: &PanelState, right: &PanelState) -> Self {
        let empty = || TabSet {
            tabs: Vec::new(),
            active: 0,
        };
        let mut tabs = Self {
            sets: [empty(), empty()],
            next_id: 1,
        };
        for (panel, state) in [(PanelId::Left, left), (PanelId::Right, right)] {
            let tab = tabs.new_tab(state.path.clone(), state.sort, state.mode);
            tabs.sets[panel.index()].tabs.push(tab);
        }
        tabs
    }

    #[must_use]
    pub fn set(&self, panel: PanelId) -> &TabSet {
        &self.sets[panel.index()]
    }

    pub fn set_mut(&mut self, panel: PanelId) -> &mut TabSet {
        &mut self.sets[panel.index()]
    }

    /// A fresh unlocked tab on `path`.
    pub fn new_tab(&mut self, path: VfsPath, sort: SortOrder, mode: ListMode) -> Tab {
        Tab {
            id: TabId(self.next_id_take()),
            history: History::new(path.clone()),
            path,
            sort,
            mode,
            locked: false,
            cursor: None,
        }
    }

    /// A copy of `tab` under a new id, unlocked as Total Commander copies a tab.
    pub fn copy_tab(&mut self, tab: &Tab) -> Tab {
        Tab {
            id: TabId(self.next_id_take()),
            locked: false,
            ..tab.clone()
        }
    }

    fn next_id_take(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Replaces a panel's tabs with ones read back, under new ids.
    pub fn replace(&mut self, panel: PanelId, mut set: TabSet) {
        for tab in &mut set.tabs {
            tab.id = TabId(self.next_id_take());
        }
        self.sets[panel.index()] = set;
    }

    /// Exchanges the two panels' tabs (Ctrl+Shift+U).
    pub fn swap(&mut self) {
        self.sets.swap(0, 1);
    }

    /// Whether the panel's active tab is locked.
    #[must_use]
    pub fn locked(&self, panel: PanelId) -> bool {
        self.set(panel).active_tab().locked
    }

    /// The panel's active tab now shows `path`: a step of its history.
    pub fn visited(&mut self, panel: PanelId, path: &VfsPath) {
        self.set_mut(panel).active_tab_mut().history.visit(path);
    }

    /// Copies the panel's live state into its active tab, before the panel shows another tab or
    /// the tabs are read or stored.
    pub fn sync(&mut self, panel: PanelId, state: &PanelState) {
        let tab = self.set_mut(panel).active_tab_mut();
        tab.path.clone_from(&state.path);
        tab.sort = state.sort;
        tab.mode = state.mode;
        tab.cursor = state.cursor_entry().map(|entry| entry.name.clone());
    }

    /// Adds `tab` right after the active one and makes it active; its index.
    pub fn insert_active(&mut self, panel: PanelId, tab: Tab) -> usize {
        let set = self.set_mut(panel);
        let at = set.active + 1;
        set.tabs.insert(at, tab);
        set.active = at;
        at
    }

    /// Adds `tab` after the last one, leaving the active tab as it is.
    pub fn push(&mut self, panel: PanelId, tab: Tab) {
        self.set_mut(panel).tabs.push(tab);
    }

    /// Makes the tab at `index` active.
    pub fn activate(&mut self, panel: PanelId, index: usize) {
        let set = self.set_mut(panel);
        set.active = index.min(set.tabs.len() - 1);
    }

    /// Removes the tab at `index` unless it is the last one; the removed tab. The active tab
    /// stays active; when it was the one removed, its right neighbour is, else its left one.
    pub fn remove(&mut self, panel: PanelId, index: usize) -> Option<Tab> {
        let set = self.set_mut(panel);
        if set.tabs.len() < 2 || index >= set.tabs.len() {
            return None;
        }
        let tab = set.tabs.remove(index);
        if index < set.active || set.active == set.tabs.len() {
            set.active -= 1;
        }
        Some(tab)
    }

    /// Moves the tab `tab` to place `to` (the last place when beyond it); the shown tab stays
    /// shown. The place it now has; `None` when the panel has no such tab.
    pub fn move_tab(&mut self, panel: PanelId, tab: TabId, to: usize) -> Option<usize> {
        let set = self.set_mut(panel);
        let from = set.index_of(tab)?;
        let shown = set.tabs[set.active].id;
        let to = to.min(set.tabs.len() - 1);
        let moved = set.tabs.remove(from);
        set.tabs.insert(to, moved);
        set.active = set.index_of(shown).unwrap_or(to);
        Some(to)
    }

    /// The panel's tabs as commands report them; the active one as `state` shows it.
    #[must_use]
    pub fn views(&self, panel: PanelId, state: &PanelState) -> Vec<TabView> {
        let set = self.set(panel);
        let paths: Vec<&VfsPath> = set
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                if index == set.active {
                    &state.path
                } else {
                    &tab.path
                }
            })
            .collect();
        titles(&paths)
            .into_iter()
            .zip(set.tabs.iter().zip(paths))
            .enumerate()
            .map(|(index, (title, (tab, path)))| TabView {
                id: tab.id,
                title,
                path: path.clone(),
                locked: tab.locked,
                active: index == set.active,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "set_tests.rs"]
mod tests;
