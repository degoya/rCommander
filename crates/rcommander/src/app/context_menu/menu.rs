//! What the context menu offers and for which entries, its keyboard and where it sits - pure
//! data, tested without a window.
//!
//! The entries a menu is for follow the Explorer: a right click on a selected entry takes the
//! whole selection, on an entry that is not selected that entry alone (the selection is
//! dropped, as the Explorer does), on `..` or beside the rows the folder itself. Shift+F10 and
//! the context-menu key take the selection, or the entry under the cursor when nothing is
//! selected.
//!
//! rCommander's own entries are its keys: each runs exactly what its key runs, through the
//! keymap - the key the person moved it to, if so (RC-100-21) -, and shows only while it has one - so an entry appears by itself when
//! the job that brings its command swaps the key's reservation for it (F5 copy, RC-010-07;
//! Alt+Enter properties, RC-020-06). For entries the menu also copies their names or full paths
//! to the clipboard, as Total Commander's `cm_CopyNamesToClip` and `cm_CopyFullNamesToClip` do,
//! which have no key of their own there (`clipboard.copy_names`). Checksum files only also get
//! "Verify checksums", Ctrl+Shift+F10's check (RC-020-10).

use iced::{Point, Rectangle};
use rc_engine::{Chord, NameFormat, PanelId};

use crate::{
    app::{
        menu_letter::{self, Letter},
        panel::Pane,
    },
    i18n::t,
    theme,
};

/// The width of rCommander's own menu, in logical pixels.
pub const MENU_WIDTH: f32 = 280.0;
/// The space above and below the menu's entries inside its card.
pub const MENU_PADDING: f32 = 4.0;
/// How far right of the list's edge the menu starts, past the cursor's pill.
const INDENT: f32 = 24.0;
/// The list's own padding above its first row (`file_list.rs`).
const LIST_TOP: f32 = 2.0;

/// What the menu is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The panel's folder itself: "New", "Paste" in the Explorer's menu.
    Folder,
    /// These entries of the panel's folder, by name, in the listing's order.
    Entries(Vec<String>),
}

/// The commands rCommander puts into the menu, each the command of a Total Commander key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Open,
    View,
    Edit,
    Copy,
    Move,
    Rename,
    Delete,
    Properties,
    NewFolder,
    Refresh,
    /// For checksum files only (RC-020-10).
    Verify,
}

impl Action {
    /// The menu for entries without the Explorer's: Total Commander's file commands.
    const FOR_ENTRIES: [Self; 8] = [
        Self::Open,
        Self::View,
        Self::Edit,
        Self::Copy,
        Self::Move,
        Self::Rename,
        Self::Delete,
        Self::Properties,
    ];
    /// Above the Explorer's entries: what Total Commander does better than the shell.
    const OVER_SHELL: [Self; 3] = [Self::Copy, Self::Rename, Self::Properties];
    /// The menu of the folder without the Explorer's.
    const FOR_FOLDER: [Self; 2] = [Self::NewFolder, Self::Refresh];

    /// The key whose command the entry runs.
    fn keys(self) -> &'static str {
        match self {
            Self::Open => "Enter",
            Self::View => "F3",
            Self::Edit => "F4",
            Self::Copy => "F5",
            Self::Move => "F6",
            Self::Rename => "Shift+F6",
            Self::Delete => "F8",
            Self::Properties => "Alt+Enter",
            Self::NewFolder => "F7",
            Self::Refresh => "Ctrl+R",
            Self::Verify => "Ctrl+Shift+F10",
        }
    }

    /// The entry's label; above the Explorer's own "Properties" rCommander's say whose they are.
    fn label(self, over_shell: bool) -> &'static str {
        match self {
            Self::Open => t!("context_menu.open"),
            Self::View => t!("context_menu.view"),
            Self::Edit => t!("context_menu.edit"),
            Self::Copy => t!("context_menu.copy"),
            Self::Move => t!("context_menu.move"),
            Self::Rename => t!("context_menu.rename"),
            Self::Delete => t!("context_menu.delete"),
            Self::Properties if over_shell => t!("context_menu.own_properties"),
            Self::Properties => t!("context_menu.properties"),
            Self::NewFolder => t!("context_menu.new_folder"),
            Self::Refresh => t!("context_menu.refresh"),
            Self::Verify => t!("context_menu.verify_checksums"),
        }
    }
}

/// What one of rCommander's entries runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// What this key runs, as if pressed.
    Key(Chord),
    /// The names or full paths of the selection, or of the entry under the cursor, to the
    /// clipboard, one line each.
    CopyNames(NameFormat),
}

/// One of rCommander's entries: its label and what it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: &'static str,
    pub choice: Choice,
}

impl Item {
    /// The key shown beside the label; none for an entry without one.
    pub fn key(&self) -> Option<&Chord> {
        match &self.choice {
            Choice::Key(chord) => Some(chord),
            Choice::CopyNames(_) => None,
        }
    }
}

/// The clipboard entries under the keys' entries, for entries only.
const COPY: [NameFormat; 2] = [NameFormat::Names, NameFormat::FullPaths];

/// rCommander's entries for `target`, above the Explorer's (`over_shell`) or alone: those whose
/// command still has a key - `key` names the key that runs now what the entry's standard key runs
/// in the standard keymap, none when it has none (RC-100-21) - "Verify checksums" too when the
/// entries are all checksum files (`checksums`) -, then for entries copying their names and full
/// paths.
pub fn items(
    target: &Target,
    over_shell: bool,
    checksums: bool,
    key: impl Fn(&Chord) -> Option<Chord>,
) -> Vec<Item> {
    let actions: &[Action] = match (target, over_shell) {
        (Target::Folder, true) => &[],
        (Target::Folder, false) => &Action::FOR_FOLDER,
        (Target::Entries(_), true) => &Action::OVER_SHELL,
        (Target::Entries(_), false) => &Action::FOR_ENTRIES,
    };
    let verify = (checksums && matches!(target, Target::Entries(_))).then_some(Action::Verify);
    let keys = actions.iter().copied().chain(verify).filter_map(|action| {
        let standard: Chord = action.keys().parse().ok()?;
        let chord = key(&standard)?;
        Some(Item {
            label: action.label(over_shell),
            choice: Choice::Key(chord),
        })
    });
    let copies: &[NameFormat] = match target {
        Target::Folder => &[],
        Target::Entries(_) => &COPY,
    };
    let copies = copies.iter().map(|&format| Item {
        label: copy_label(format),
        choice: Choice::CopyNames(format),
    });
    keys.chain(copies).collect()
}

/// The label of a clipboard entry.
fn copy_label(format: NameFormat) -> &'static str {
    match format {
        NameFormat::FullPaths => t!("context_menu.copy_full_path"),
        _ => t!("context_menu.copy_name"),
    }
}

/// The standard key of rCommander's rename in place, which the Explorer's "Rename" runs - through
/// the key it has now: the shell renames in place only inside an Explorer window.
pub fn rename_chord() -> Chord {
    Action::Rename
        .keys()
        .parse()
        .expect("Shift+F6 is a chord (menu_tests)")
}

/// What a right click on `row` of `pane` opens the menu for, and whether the selection is
/// dropped first (an entry outside it was clicked).
pub fn clicked(pane: &Pane, row: usize) -> (Target, bool) {
    let entry = pane
        .entry_index(row)
        .and_then(|index| pane.entries().get(index));
    match entry {
        None => (Target::Folder, false),
        Some(entry) if pane.state.selection.contains(&entry.name) => {
            (Target::Entries(pane.chosen_names()), false)
        }
        Some(entry) => (
            Target::Entries(vec![entry.name.clone()]),
            !pane.state.selection.is_empty(),
        ),
    }
}

/// What Shift+F10 opens the menu for: the selection, else the entry under the cursor, else -
/// on `..` or in an empty folder - the folder.
pub fn at_cursor(pane: &Pane) -> Target {
    let names = pane.chosen_names();
    if names.is_empty() {
        Target::Folder
    } else {
        Target::Entries(names)
    }
}

/// An open menu: whose, for what, and where.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    pub panel: PanelId,
    pub target: Target,
    /// The listing's row it opens at: the clicked one, the cursor's, or one past the last for a
    /// click beside the rows.
    pub row: usize,
    /// Opened by Shift+F10 or the context-menu key rather than a click.
    pub by_key: bool,
    /// rCommander's entries.
    pub items: Vec<Item>,
    /// The entry the keyboard is on.
    pub cursor: usize,
    /// The panel's list in the window, once measured; the menu is drawn from then on.
    pub list: Option<Rectangle>,
    /// The Explorer's menu is wanted (a local folder on Windows); rCommander's is the fallback.
    pub shell: bool,
}

/// What a key does to the open menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// It stays open; the cursor may have moved.
    Stay,
    Close,
    /// The menu closes and this entry runs.
    Run(Choice),
}

impl ContextMenu {
    /// A key while the menu is open: arrows, Home and End move, Enter or Space take the entry,
    /// a letter takes the one entry starting with it or moves to the next of several
    /// (`menu_letter`), Escape and the menu keys close it; every other key is swallowed, as by a
    /// Win32 menu.
    pub fn key(&mut self, chord: &Chord) -> MenuAction {
        if menu_letter::is_letter(chord) {
            let labels = self.items.iter().map(|item| Some(item.label));
            return match menu_letter::letter(&chord.key, labels, self.cursor) {
                Letter::None => MenuAction::Stay,
                Letter::Move(index) => {
                    self.cursor = index;
                    MenuAction::Stay
                }
                Letter::Take(index) => MenuAction::Run(self.items[index].choice.clone()),
            };
        }
        let plain = chord.is_plain();
        let last = self.items.len().saturating_sub(1);
        match chord.key.as_str() {
            "Escape" | "ContextMenu" => MenuAction::Close,
            "F10" if chord.shift => MenuAction::Close,
            "ArrowUp" if plain => {
                self.cursor = self.cursor.checked_sub(1).unwrap_or(last);
                MenuAction::Stay
            }
            "ArrowDown" if plain => {
                self.cursor = if self.cursor >= last {
                    0
                } else {
                    self.cursor + 1
                };
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
            "Enter" | "Space" if plain => self
                .items
                .get(self.cursor)
                .map_or(MenuAction::Close, |item| {
                    MenuAction::Run(item.choice.clone())
                }),
            _ => MenuAction::Stay,
        }
    }
}

/// Where a menu of `entries` rows opens for `row` of `pane` whose list is at `list` in the
/// window: under the row, or above it when there is no room below, inside the list; in the
/// brief view in the row's column. With no entries the point is the row's lower left corner,
/// where the Explorer's menu opens for the keyboard. The list's rows are as high as its font
/// makes them (RC-010-17).
pub fn place(entries: usize, pane: &Pane, row: usize, list: Rectangle) -> Point {
    let offset = row.saturating_sub(pane.top());
    let (column, line) = if pane.brief() {
        let rows = pane.page_rows().max(1);
        (offset / rows, offset % rows)
    } else {
        (0, offset)
    };
    let height = if entries == 0 {
        0.0
    } else {
        entries as f32 * theme::ROW_HEIGHT + 2.0 * MENU_PADDING
    };
    let row_height = theme::fonts::list().row_height();
    let below = LIST_TOP + (line + 1) as f32 * row_height;
    let y = if entries == 0 || below + height <= list.height {
        below
    } else {
        (below - row_height - height).max(0.0)
    };
    // The brief view's columns share the list's width (`file_list.rs`): each is wider than
    // the narrowest one they are counted by (RC-080-44).
    let column_width = list.width / pane.columns().max(1) as f32;
    let x = (INDENT + column as f32 * column_width)
        .min(list.width - MENU_WIDTH)
        .max(0.0);
    Point::new(list.x + x, list.y + y)
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
