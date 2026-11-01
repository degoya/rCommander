//! The drive menu's keyboard, as Total Commander's Alt+F1 menu: arrows, Home and End move,
//! Enter or Space take the entry, a drive's letter takes that drive at once, another letter
//! moves to the next entry starting with it, Escape closes. Alt+F1 and Alt+F2 close the menu of
//! their own panel or move to the other panel's. Under the drives stand the network drive actions
//! (`actions.rs`), reached by the arrows like a drive. Shift+F10 and the context-menu key open the
//! context menu of the drive under the cursor, as a right click on it does (RC-080-16).
//!
//! What the menu lists depends on where it was opened (RC-100-62, [`Scope`]): from the drive line
//! and the keys every entry and the actions; from `»` the bar's items it had no room for; from a
//! folder's button that folder's servers. The keys work on the list shown, nothing else.

use rc_engine::{Chord, Drive, DriveGroup, PanelId};

use super::{
    actions::DriveAction,
    overflow::{BarItem, bar_items},
};

/// What the menu lists, by where it was opened.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Scope {
    /// Every entry, then the network drive actions: the drive line, Alt+F1 and Alt+F2.
    #[default]
    All,
    /// `»`: the bar's items from this one on ([`bar_items`]), which the bar had no room for; a
    /// folder there is a heading with its servers.
    Overflow(usize),
    /// A folder's button: the servers of the folder with this path from the root, its
    /// subfolders' under their headings (RC-100-63: any marked folder, not only a top-level one).
    Folder(Vec<String>),
}

/// The open menu: whose it is, what it lists and where its cursor stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveMenu {
    pub panel: PanelId,
    /// The stop the cursor is on: the entries as listed, then the actions.
    pub cursor: usize,
    /// The actions under the drives; listed under [`Scope::All`] only.
    pub actions: Vec<DriveAction>,
    pub scope: Scope,
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

/// A place of the cursor: a drive by its index among the entries, or an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Drive(usize),
    Action(DriveAction),
}

impl DriveMenu {
    /// The menu of `panel` with every entry, its cursor on the drive `current` when it is listed.
    #[cfg(test)]
    pub fn new(panel: PanelId, drives: &[Drive], current: Option<&str>) -> Self {
        Self::scoped(panel, Scope::All, drives, current)
    }

    /// The menu of `panel` listing `scope`, its cursor on the drive `current` when it is listed,
    /// else on the first entry.
    pub fn scoped(panel: PanelId, scope: Scope, drives: &[Drive], current: Option<&str>) -> Self {
        let mut menu = Self {
            panel,
            cursor: 0,
            actions: Vec::new(),
            scope,
        };
        let cursor = current.and_then(|id| {
            stops(drives, &menu)
                .iter()
                .position(|stop| matches!(stop, Stop::Drive(index) if drives[*index].id == id))
        });
        menu.cursor = cursor.unwrap_or(0);
        menu
    }

    /// The menu with `actions` under its drives.
    pub fn with_actions(mut self, actions: Vec<DriveAction>) -> Self {
        self.actions = actions;
        self
    }

    /// Keeps the cursor on an entry after the list of `drives` changed.
    pub fn clamp(&mut self, drives: &[Drive]) {
        let count = stops(drives, self).len();
        self.cursor = self.cursor.min(count.saturating_sub(1));
    }

    /// What `chord` does with `drives` listed.
    pub fn key(&mut self, chord: &Chord, drives: &[Drive]) -> MenuAction {
        let stops = stops(drives, self);
        let last = stops.len().saturating_sub(1);
        let plain = !chord.ctrl && !chord.alt;
        match chord.key.as_str() {
            "Escape" => MenuAction::Close,
            "ContextMenu" => self.context(drives, &stops),
            "F10" if chord.shift && !chord.ctrl && !chord.alt => self.context(drives, &stops),
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
            "Enter" | "Space" if plain => match stops.get(self.cursor) {
                Some(Stop::Drive(index)) => MenuAction::Pick(drives[*index].id.clone()),
                Some(Stop::Action(action)) => MenuAction::Run(*action),
                None => MenuAction::Stay,
            },
            key if plain && key.chars().count() == 1 => self.letter(key, drives, &stops),
            _ => MenuAction::Stay,
        }
    }

    /// The context menu of the drive under the cursor; nothing on an action.
    fn context(&self, drives: &[Drive], stops: &[Stop]) -> MenuAction {
        match stops.get(self.cursor) {
            Some(Stop::Drive(index)) => MenuAction::Context(drives[*index].id.clone()),
            _ => MenuAction::Stay,
        }
    }

    fn toggle(&self, panel: PanelId) -> MenuAction {
        if panel == self.panel {
            MenuAction::Close
        } else {
            MenuAction::Switch(panel)
        }
    }

    /// A listed drive's letter takes its drive; any other character moves the cursor to the next
    /// listed entry whose label starts with it, wrapping around.
    fn letter(&mut self, key: &str, drives: &[Drive], stops: &[Stop]) -> MenuAction {
        let id = format!("{}:", key.to_uppercase());
        let lettered = stops.iter().find_map(|stop| match stop {
            Stop::Drive(index)
                if drives[*index].group == DriveGroup::Drives && drives[*index].id == id =>
            {
                Some(drives[*index].id.clone())
            }
            _ => None,
        });
        if let Some(id) = lettered {
            return MenuAction::Pick(id);
        }
        let count = stops.len();
        let starts = |stop: &Stop| match stop {
            Stop::Drive(index) => drives[*index].label.to_lowercase().starts_with(key),
            Stop::Action(_) => false,
        };
        if let Some(next) = (1..=count)
            .map(|step| (self.cursor + step) % count)
            .find(|&at| starts(&stops[at]))
        {
            self.cursor = next;
        }
        MenuAction::Stay
    }
}

/// One line of the menu as it is drawn: a group's heading, a folder's heading (its path from the
/// level shown), an entry, the actions' heading, an action. An entry and an action carry their
/// stop, the place of the cursor on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuLine<'a> {
    Group(DriveGroup),
    Folder(&'a [String]),
    /// The drive at index `drive` of the entries; `indent` under a folder's heading.
    Entry {
        stop: usize,
        drive: usize,
        indent: bool,
    },
    Actions,
    Action(usize, DriveAction),
}

/// The menu's lines as built, with the stops counted.
#[derive(Default)]
struct Lines<'a> {
    lines: Vec<MenuLine<'a>>,
    stops: usize,
}

impl<'a> Lines<'a> {
    /// The drive at `index` with the folder path `path` shown for it: under a new heading when
    /// the path is not `last`'s, indented under a heading.
    fn entry(&mut self, index: usize, path: &'a [String], last: &mut &'a [String]) {
        if path != *last {
            *last = path;
            if !path.is_empty() {
                self.lines.push(MenuLine::Folder(path));
            }
        }
        self.push(index, !path.is_empty());
    }

    /// The drive at `index` on a line of its own, under no heading.
    fn plain(&mut self, index: usize) {
        self.push(index, false);
    }

    fn push(&mut self, drive: usize, indent: bool) {
        self.lines.push(MenuLine::Entry {
            stop: self.stops,
            drive,
            indent,
        });
        self.stops += 1;
    }

    /// The servers of the folder at `path` and below, their paths shown from level `skip`.
    fn folder(&mut self, drives: &'a [Drive], path: &[String], skip: usize) {
        let mut last: &[String] = &[];
        for (index, drive) in drives.iter().enumerate() {
            if !path.is_empty() && drive.folder.starts_with(path) {
                self.entry(
                    index,
                    drive.folder.get(skip..).unwrap_or_default(),
                    &mut last,
                );
            }
        }
    }
}

/// The lines of `menu` over `drives`. Every entry ([`Scope::All`]): each group and each folder of
/// servers under a heading, then the actions under theirs. `»`: the hidden items in the bar's
/// order, a folder as a heading over its servers. A folder's button: its servers, those of a
/// subfolder under the subfolder's heading.
pub fn lines<'a>(drives: &'a [Drive], menu: &DriveMenu) -> Vec<MenuLine<'a>> {
    let mut built = Lines::default();
    match &menu.scope {
        Scope::All => {
            let mut group = None;
            let mut last: &[String] = &[];
            for (index, drive) in drives.iter().enumerate() {
                if group != Some(drive.group) {
                    group = Some(drive.group);
                    built.lines.push(MenuLine::Group(drive.group));
                }
                built.entry(index, &drive.folder, &mut last);
            }
            if !menu.actions.is_empty() {
                built.lines.push(MenuLine::Actions);
            }
            for action in &menu.actions {
                built.lines.push(MenuLine::Action(built.stops, *action));
                built.stops += 1;
            }
        }
        Scope::Overflow(shown) => {
            for item in bar_items(drives).into_iter().skip(*shown) {
                match item {
                    BarItem::Gap => {}
                    // Headed by its own name: the levels above it are left out.
                    BarItem::Folder(path) => {
                        built.folder(drives, path, path.len().saturating_sub(1))
                    }
                    BarItem::Drive(index) => built.plain(index),
                }
            }
        }
        Scope::Folder(path) => built.folder(drives, path, path.len()),
    }
    built.lines
}

/// The stops of `menu` over `drives`, in their order.
fn stops(drives: &[Drive], menu: &DriveMenu) -> Vec<Stop> {
    lines(drives, menu)
        .into_iter()
        .filter_map(|line| match line {
            MenuLine::Entry { drive, .. } => Some(Stop::Drive(drive)),
            MenuLine::Action(_, action) => Some(Stop::Action(action)),
            _ => None,
        })
        .collect()
}

/// The line the cursor of `menu` is drawn on, and how many lines there are: what the menu
/// scrolls by.
pub fn cursor_line(drives: &[Drive], menu: &DriveMenu) -> (usize, usize) {
    let lines = lines(drives, menu);
    let at = lines
        .iter()
        .position(|line| match line {
            MenuLine::Entry { stop, .. } | MenuLine::Action(stop, _) => *stop == menu.cursor,
            _ => false,
        })
        .unwrap_or(0);
    (at, lines.len())
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
