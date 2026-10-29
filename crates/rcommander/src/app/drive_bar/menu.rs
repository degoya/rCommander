//! The drive menu's keyboard, as Total Commander's Alt+F1 menu: arrows, Home and End move,
//! Enter or Space take the entry, a drive's letter takes that drive at once, another letter
//! moves to the next entry starting with it, Escape closes. Alt+F1 and Alt+F2 close the menu of
//! their own panel or move to the other panel's. Under the drives stand the network drive actions
//! (`actions.rs`), reached by the arrows like a drive. Shift+F10 and the context-menu key open the
//! context menu of the drive under the cursor, as a right click on it does (RC-080-16). The menu
//! lists every entry, so it is also the keyboard's way to the drives the bar's `»` holds.

use rc_engine::{Chord, Drive, DriveGroup, PanelId};

use super::actions::DriveAction;

/// The open menu: whose it is and where its cursor stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveMenu {
    pub panel: PanelId,
    /// Index into the drive bar's entries, then into `actions`.
    pub cursor: usize,
    /// The actions under the drives.
    pub actions: Vec<DriveAction>,
}

/// What a key does to the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// The menu stays open (the cursor may have moved).
    Stay,
    Close,
    /// The entry with this id is taken; the menu closes.
    Pick(String),
    /// The action is run; the menu closes.
    Run(DriveAction),
    /// The other panel's menu opens instead.
    Switch(PanelId),
    /// The context menu of the entry with this id opens.
    Context(String),
}

impl DriveMenu {
    /// The menu of `panel`, its cursor on the drive `current` when it is listed.
    pub fn new(panel: PanelId, drives: &[Drive], current: Option<&str>) -> Self {
        let cursor = current
            .and_then(|id| drives.iter().position(|drive| drive.id == id))
            .unwrap_or(0);
        Self {
            panel,
            cursor,
            actions: Vec::new(),
        }
    }

    /// The menu with `actions` under its drives.
    pub fn with_actions(mut self, actions: Vec<DriveAction>) -> Self {
        self.actions = actions;
        self
    }

    /// Keeps the cursor on an entry after the list of `len` drives changed.
    pub fn clamp(&mut self, len: usize) {
        self.cursor = self
            .cursor
            .min((len + self.actions.len()).saturating_sub(1));
    }

    /// What `chord` does with `drives` listed.
    pub fn key(&mut self, chord: &Chord, drives: &[Drive]) -> MenuAction {
        let last = (drives.len() + self.actions.len()).saturating_sub(1);
        let plain = !chord.ctrl && !chord.alt;
        match chord.key.as_str() {
            "Escape" => MenuAction::Close,
            "ContextMenu" => self.context(drives),
            "F10" if chord.shift && !chord.ctrl && !chord.alt => self.context(drives),
            "F1" if chord.alt => self.toggle(PanelId::Left),
            "F2" if chord.alt => self.toggle(PanelId::Right),
            "ArrowUp" if plain => {
                self.cursor = self.cursor.saturating_sub(1);
                MenuAction::Stay
            }
            "ArrowDown" if plain => {
                self.cursor = (self.cursor + 1).min(last);
                MenuAction::Stay
            }
            "Home" | "PageUp" if plain => {
                self.cursor = 0;
                MenuAction::Stay
            }
            "End" | "PageDown" if plain => {
                self.cursor = last;
                MenuAction::Stay
            }
            "Enter" | "Space" if plain => match drives.get(self.cursor) {
                Some(drive) => MenuAction::Pick(drive.id.clone()),
                None => self
                    .actions
                    .get(self.cursor - drives.len())
                    .map_or(MenuAction::Stay, |action| MenuAction::Run(*action)),
            },
            key if plain && key.chars().count() == 1 => self.letter(key, drives),
            _ => MenuAction::Stay,
        }
    }

    /// The context menu of the drive under the cursor; nothing on an action.
    fn context(&self, drives: &[Drive]) -> MenuAction {
        drives.get(self.cursor).map_or(MenuAction::Stay, |drive| {
            MenuAction::Context(drive.id.clone())
        })
    }

    fn toggle(&self, panel: PanelId) -> MenuAction {
        if panel == self.panel {
            MenuAction::Close
        } else {
            MenuAction::Switch(panel)
        }
    }

    /// A drive letter takes its drive; any other character moves the cursor to the next entry
    /// whose label starts with it, wrapping around.
    fn letter(&mut self, key: &str, drives: &[Drive]) -> MenuAction {
        let id = format!("{}:", key.to_uppercase());
        if let Some(drive) = drives
            .iter()
            .find(|drive| drive.group == DriveGroup::Drives && drive.id == id)
        {
            return MenuAction::Pick(drive.id.clone());
        }
        let count = drives.len();
        if let Some(next) = (1..=count)
            .map(|step| (self.cursor + step) % count)
            .find(|&index| drives[index].label.to_lowercase().starts_with(key))
        {
            self.cursor = next;
        }
        MenuAction::Stay
    }
}

/// One line of the menu as it is drawn: a group's heading, a folder's heading, an entry by its
/// index, the actions' heading, an action by its cursor index (after the entries).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuLine<'a> {
    Group(DriveGroup),
    Folder(&'a [String]),
    Entry(usize),
    Actions,
    Action(usize, DriveAction),
}

/// The menu's lines: each group and each folder of servers under a heading, then the actions
/// under theirs.
pub fn lines<'a>(drives: &'a [Drive], actions: &[DriveAction]) -> Vec<MenuLine<'a>> {
    let mut lines = Vec::with_capacity(drives.len() + actions.len() + 5);
    let mut group = None;
    let mut folder: &[String] = &[];
    for (index, drive) in drives.iter().enumerate() {
        if group != Some(drive.group) {
            group = Some(drive.group);
            lines.push(MenuLine::Group(drive.group));
        }
        if drive.folder.as_slice() != folder {
            folder = &drive.folder;
            if !folder.is_empty() {
                lines.push(MenuLine::Folder(folder));
            }
        }
        lines.push(MenuLine::Entry(index));
    }
    if !actions.is_empty() {
        lines.push(MenuLine::Actions);
    }
    for (index, action) in actions.iter().enumerate() {
        lines.push(MenuLine::Action(drives.len() + index, *action));
    }
    lines
}

/// The line entry `cursor` is drawn on, and how many lines there are: what the menu scrolls by.
pub fn cursor_line(drives: &[Drive], actions: &[DriveAction], cursor: usize) -> (usize, usize) {
    let lines = lines(drives, actions);
    let at = lines
        .iter()
        .position(|line| match line {
            MenuLine::Entry(index) | MenuLine::Action(index, _) => *index == cursor,
            _ => false,
        })
        .unwrap_or(0);
    (at, lines.len())
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
