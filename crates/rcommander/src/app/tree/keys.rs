//! The keys of the folder trees (RC-100-88). While a tree in the main window has the keyboard -
//! the tree in the active panel's place, or the active panel's separate tree after Shift+Tab or
//! a click in it - the cursor keys move through it (Right unfolds or steps in, Left folds or
//! steps out), Enter leads its panel to the folder at once, F2 reads the folder again and typed
//! letters jump. Every other key works as in the file list, but those that work on the panel's
//! entries: no file operation from a tree in this stage (the job file's "out of scope" list).
//!
//! Alt+F10 (`tree_dialog`) opens the tree dialog, Shift+Tab (`tree_focus`) moves the keyboard
//! between the file list and its separate tree, as in Total Commander; both are window keys of
//! the keymap.

use std::time::Instant;

use rc_engine::{Chord, Command, PanelView, tree::SeparateTree};

use super::{Change, DirTree, TreeCall, TreeId, model::Step};
use crate::app::{App, Effect};

/// The window key of the tree dialog (Alt+F10).
pub const DIALOG_KEY: &str = "tree_dialog";
/// The window key that moves the keyboard between a list and its separate tree (Shift+Tab).
pub const FOCUS_KEY: &str = "tree_focus";

/// The panel commands a key still runs from a tree: those that change what the panel shows,
/// none that works on its entries.
const PANEL_KEPT: &[&str] = &[
    "panel.show_tree",
    "panel.cycle_separate_tree",
    "panel.switch_active",
    "panel.set_view",
    "panel.change_drive",
    "panel.drive_menu",
    "panel.toggle_terminal",
    "panel.show_files",
    "panel.toggle_hidden",
];

/// The groups whose commands work on a panel's entries.
const ON_ENTRIES: &[&str] = &[
    "panel.",
    "file.",
    "ops.",
    "archive.",
    "clipboard.",
    "checksum.",
    "multi_rename.",
    "compare.",
];

/// The step a plain cursor key makes in a tree.
pub(super) fn step_of(chord: &Chord) -> Option<Step> {
    if !chord.is_plain() {
        return None;
    }
    Some(match chord.key.as_str() {
        "ArrowUp" => Step::Up,
        "ArrowDown" => Step::Down,
        "PageUp" => Step::PageUp,
        "PageDown" => Step::PageDown,
        "Home" => Step::Home,
        "End" => Step::End,
        "ArrowRight" => Step::Open,
        "ArrowLeft" => Step::Close,
        _ => return None,
    })
}

/// Whether the key of `command` runs while a tree has the keyboard.
pub fn passes(command: &Command) -> bool {
    let id = command.id();
    PANEL_KEPT.contains(&id) || !ON_ENTRIES.iter().any(|group| id.starts_with(group))
}

/// Whether the window key `name` runs while a tree has the keyboard: not the clipboard's nor
/// the command line's, which take the panel's entries.
fn window_passes(name: &str) -> bool {
    !name.starts_with("clipboard_") && !name.starts_with("cmdline_")
}

impl App {
    /// The tree of the main window that has the keyboard: the active panel's separate tree while
    /// it has the focus, else the tree in the active panel's place; none while a prompt is open.
    pub(in crate::app) fn keyed_tree(&self) -> Option<TreeId> {
        if self.prompt.is_some() {
            return None;
        }
        let active = self.active;
        if self.tree.focus && self.tree.separate[active.index()].is_some() {
            return Some(TreeId::Separate(active));
        }
        let shows = self.pane(active).state.view == PanelView::Tree;
        (shows && self.tree.panels[active.index()].is_some()).then_some(TreeId::Panel(active))
    }

    /// A key while a tree of the main window has the keyboard.
    pub(in crate::app) fn keyed_tree_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let Some(id) = self.keyed_tree() else {
            return Vec::new();
        };
        if let Some(effects) = self.tree_nav_key(id, chord) {
            return effects;
        }
        // Any other key as in the file list, unless it works on the panel's entries.
        let view = PanelView::Files;
        if self
            .keymap
            .window_key(chord, view)
            .is_some_and(|name| !window_passes(name))
            || self
                .keymap
                .lookup(chord, view)
                .is_some_and(|command| !passes(command))
        {
            return Vec::new();
        }
        self.on_key(chord.clone(), false)
    }

    /// The keys every tree takes: the cursor keys, Enter and F2; `None` for any other.
    pub(super) fn tree_nav_key(&mut self, id: TreeId, chord: &Chord) -> Option<Vec<Effect>> {
        if let Some(step) = step_of(chord) {
            let change = self
                .tree_mut(id)
                .map_or(Change::Nothing, |tree| tree.step(step));
            return Some(self.tree_changed(id, change));
        }
        if !chord.is_plain() {
            return None;
        }
        match chord.key.as_str() {
            "Enter" => Some(self.tree_enter(id)),
            "F2" => Some(self.tree_reread(id)),
            _ => None,
        }
    }

    /// Letters typed while tree `id` has the keyboard: the cursor jumps.
    pub(in crate::app) fn tree_typed(&mut self, id: TreeId, text: &str) -> Vec<Effect> {
        let moved = self
            .tree_mut(id)
            .is_some_and(|tree| tree.jump(text, Instant::now()));
        if moved {
            self.tree_changed(id, Change::Cursor)
        } else {
            Vec::new()
        }
    }

    /// Letters typed while a tree of the main window has the keyboard.
    pub(in crate::app) fn keyed_tree_typed(&mut self, text: &str) -> Vec<Effect> {
        match self.keyed_tree() {
            Some(id) => self.tree_typed(id, text),
            None => Vec::new(),
        }
    }

    /// F2: the folder under the cursor of tree `id` is read again.
    fn tree_reread(&mut self, id: TreeId) -> Vec<Effect> {
        let Some(path) = self.tree_mut(id).and_then(DirTree::reread) else {
            return Vec::new();
        };
        self.engine.tree_reread(&path);
        vec![Effect::Tree(TreeCall::Load { tree: id, path })]
    }

    /// The trees' window keys: Alt+F10 opens the dialog, Shift+Tab moves the keyboard; `None`
    /// for another window key.
    pub(in crate::app) fn tree_window_key(&mut self, name: &str) -> Option<Vec<Effect>> {
        match name {
            DIALOG_KEY => Some(self.open_tree_dialog()),
            FOCUS_KEY => {
                self.toggle_tree_focus();
                Some(Vec::new())
            }
            _ => None,
        }
    }

    /// Shift+Tab: the keyboard between the active panel's list and its separate tree, while it
    /// has one and shows no terminal.
    fn toggle_tree_focus(&mut self) {
        let active = self.active;
        let beside = self.tree.mode != SeparateTree::Off
            && self.tree.separate[active.index()].is_some()
            && self.pane(active).state.view != PanelView::Terminal;
        self.tree.focus = beside && !self.tree.focus;
    }
}
