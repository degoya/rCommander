//! The context menu (RC-080-02): a right click on a row or beside the rows, Shift+F10 and the
//! context-menu key (`panel.context_menu`, at the cursor's row).
//!
//! On a local folder under Windows it is the Explorer's menu, with every shell extension
//! (7-Zip, Git, "Send to", "Open with", "Properties"), and rCommander's copy, rename and
//! properties, and copying names and full paths to the clipboard, above it
//! (`platform/shell_menu.rs`). Elsewhere - a server, an archive, Linux and
//! macOS - and when the shell cannot make a menu for a path, it is rCommander's own, drawn over
//! the window (`view.rs`) with the keyboard of a Win32 menu. Which entries the menu is for and
//! what it offers is `menu.rs`.
//!
//! Opening goes in two steps: the click or key fixes what the menu is for (and puts the cursor
//! on the clicked row, dropping a selection it is not part of, in one job), then the panel's list
//! is measured, which places the menu - under the row, or for the Explorer's menu at the pointer
//! or the cursor's row. After a shell entry ran, the panel is read again. The keys beside the
//! entries, in this menu and the others rCommander draws, are `keys.rs`.

pub(super) mod keys;
mod menu;
mod view;

use std::collections::HashSet;

use iced::{
    Rectangle, Task,
    advanced::widget::{Id, Operation, operate, operation::Outcome},
};
use rc_engine::{Algorithm, Chord, Command, CursorTarget, PanelId, PanelView, VfsPath};

pub use self::menu::ContextMenu;
use self::menu::{Choice, MenuAction, Target};
pub(super) use self::view::with_menu;
use super::{App, Effect, Message, checksum::is_checksum_file, panel::Pane};
use crate::platform::shell_menu::{self, Anchor, ShellOutcome, ShellRequest};

#[derive(Debug, Clone)]
pub enum ContextMessage {
    /// A right click on a row of a panel's list.
    Row(PanelId, usize),
    /// A right click on a panel's list beside the rows.
    Beside(PanelId),
    /// Where the open menu's panel list is in the window; `None` when it is not shown.
    Measured(Option<Rectangle>),
    /// The pointer is over an entry of rCommander's menu.
    Hover(usize),
    /// A click on an entry of rCommander's menu: what it runs.
    Pick(Choice),
    /// A click beside rCommander's menu.
    Close,
    /// The Explorer's menu closed: the menu it was opened for, and how it ended.
    Shell(Box<ContextMenu>, ShellOutcome),
}

impl App {
    pub(super) fn on_context_menu(&mut self, message: ContextMessage) -> Vec<Effect> {
        match message {
            ContextMessage::Row(panel, row) => self.right_click(panel, Some(row)),
            ContextMessage::Beside(panel) => self.right_click(panel, None),
            ContextMessage::Measured(list) => self.measured(list),
            ContextMessage::Hover(index) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.cursor = index.min(menu.items.len().saturating_sub(1));
                }
                Vec::new()
            }
            ContextMessage::Pick(choice) => {
                let open = self.context_menu.as_ref().map(|open| open.panel);
                open.map_or_else(Vec::new, |panel| self.run_entry(panel, choice))
            }
            ContextMessage::Close => {
                self.context_menu = None;
                Vec::new()
            }
            ContextMessage::Shell(menu, outcome) => self.shell_closed(*menu, outcome),
        }
    }

    /// `panel.context_menu` (Shift+F10, the context-menu key): the menu of the selection, at the
    /// cursor's row.
    pub(super) fn context_menu_wanted(&mut self, panel: PanelId) -> Vec<Effect> {
        let pane = self.pane(panel);
        if pane.state.view != PanelView::Files {
            return Vec::new();
        }
        let target = menu::at_cursor(pane);
        let row = pane.cursor_row();
        self.open_menu(panel, target, row, true, Vec::new())
    }

    /// A key while rCommander's menu is open: the menu has the keyboard.
    pub(super) fn context_menu_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let Some(open) = &mut self.context_menu else {
            return Vec::new();
        };
        let panel = open.panel;
        match open.key(chord) {
            MenuAction::Stay => Vec::new(),
            MenuAction::Close => {
                self.context_menu = None;
                Vec::new()
            }
            MenuAction::Run(choice) => self.run_entry(panel, choice),
        }
    }

    /// A right click on `row`, or beside the rows: the panel becomes active and the cursor goes
    /// to the row as with a left click, a selection the row is not part of is dropped - one job,
    /// before the menu opens.
    fn right_click(&mut self, panel: PanelId, row: Option<usize>) -> Vec<Effect> {
        let mut commands = Vec::with_capacity(3);
        if panel != self.active {
            commands.push(Command::SwitchActivePanel { panel: Some(panel) });
        }
        let (target, at) = match row {
            Some(row) => {
                let (target, deselect) = menu::clicked(self.pane(panel), row);
                if deselect {
                    commands.push(Command::DeselectAll { panel: Some(panel) });
                }
                if let Some(index) = self.pane_mut(panel).point_at(row) {
                    commands.push(Command::SetCursor {
                        panel: Some(panel),
                        target: CursorTarget::Index(index),
                    });
                }
                (target, row)
            }
            None => (Target::Folder, self.pane(panel).rows()),
        };
        self.open_menu(panel, target, at, false, commands)
    }

    /// Opens the menu for `target` at `row` of `panel` once `commands` ran; the list is measured
    /// first. A menu with nothing to offer does not open.
    fn open_menu(
        &mut self,
        panel: PanelId,
        target: Target,
        row: usize,
        by_key: bool,
        commands: Vec<Command>,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        if !commands.is_empty() {
            effects.push(Effect::Dispatch(panel, commands));
        }
        let shell = shell_menu::AVAILABLE && is_local(&self.pane(panel).state.path);
        let items = self.items(panel, &target, shell);
        if !shell && items.is_empty() {
            self.context_menu = None;
            return effects;
        }
        self.context_menu = Some(ContextMenu {
            panel,
            target,
            row,
            by_key,
            items,
            cursor: 0,
            list: None,
            shell,
        });
        effects.push(Effect::Task(measure(panel)));
        effects
    }

    /// rCommander's entries for `target` of `panel` whose commands have a key, each with the key
    /// it has now.
    fn items(&self, panel: PanelId, target: &Target, over_shell: bool) -> Vec<menu::Item> {
        let checksums = checksum_files(self.pane(panel), target);
        menu::items(target, over_shell, checksums, |chord| {
            self.keymap.moved_key(chord).cloned()
        })
    }

    /// The list is measured: rCommander's menu shows, or the Explorer's opens.
    fn measured(&mut self, list: Option<Rectangle>) -> Vec<Effect> {
        let Some(list) = list else {
            // The panel shows no list any more (its terminal): no menu.
            self.context_menu = None;
            return Vec::new();
        };
        let Some(open) = &mut self.context_menu else {
            return Vec::new();
        };
        open.list = Some(list);
        if !open.shell {
            return Vec::new();
        }
        let Some(open) = self.context_menu.take() else {
            return Vec::new();
        };
        match shell_menu::show(self.shell_request(&open)) {
            Some(task) => {
                let open = Box::new(open);
                let closed = task.map(move |outcome| {
                    Message::ContextMenu(ContextMessage::Shell(open.clone(), outcome))
                });
                vec![Effect::Task(closed)]
            }
            None => {
                self.fall_back(open);
                Vec::new()
            }
        }
    }

    /// What the Explorer's menu is asked for: the entries, rCommander's own with their keys
    /// beside them (where they have one), at the pointer or - for the keyboard - under the
    /// cursor's row.
    fn shell_request(&self, open: &ContextMenu) -> ShellRequest {
        let pane = self.pane(open.panel);
        let anchor = match open.list {
            Some(list) if open.by_key => {
                let at = menu::place(0, pane, open.row, list);
                Anchor::Window { x: at.x, y: at.y }
            }
            _ => Anchor::Pointer,
        };
        let names = match &open.target {
            Target::Folder => Vec::new(),
            Target::Entries(names) => names.clone(),
        };
        let own = open
            .items
            .iter()
            .map(|item| match item.key().and_then(keys::of_chord) {
                Some(key) => format!("{}\t{key}", item.label),
                None => item.label.to_owned(),
            })
            .collect();
        ShellRequest {
            folder: pane.state.path.path.clone(),
            names,
            own,
            anchor,
        }
    }

    /// How the Explorer's menu ended.
    fn shell_closed(&mut self, open: ContextMenu, outcome: ShellOutcome) -> Vec<Effect> {
        match outcome {
            ShellOutcome::Dismissed => Vec::new(),
            ShellOutcome::Run(index) => match open.items.get(index) {
                Some(item) => self.run_entry(open.panel, item.choice.clone()),
                None => Vec::new(),
            },
            ShellOutcome::Rename => match self.keymap.moved_key(&menu::rename_chord()).cloned() {
                Some(chord) => self.run_entry(open.panel, Choice::Key(chord)),
                None => Vec::new(),
            },
            ShellOutcome::Invoked => {
                let refresh = Command::Refresh {
                    panel: Some(open.panel),
                };
                vec![Effect::Dispatch(open.panel, vec![refresh])]
            }
            ShellOutcome::Failed => {
                self.fall_back(open);
                Vec::new()
            }
        }
    }

    /// rCommander's own menu in place of the Explorer's, which could not be made.
    fn fall_back(&mut self, mut open: ContextMenu) {
        open.items = self.items(open.panel, &open.target, false);
        open.shell = false;
        open.cursor = 0;
        self.context_menu = (!open.items.is_empty()).then_some(open);
    }

    /// An entry of rCommander's for `panel`: the menu closes and its key runs, as if pressed, or
    /// the names go to the clipboard as Ctrl+Shift+C sends them (`properties.rs`).
    fn run_entry(&mut self, panel: PanelId, choice: Choice) -> Vec<Effect> {
        self.context_menu = None;
        match choice {
            Choice::Key(chord) => self.apply(Message::Key(chord)),
            Choice::CopyNames(format) => {
                let copy = Command::CopyNames {
                    panel: Some(panel),
                    format,
                };
                self.properties_command(panel, &copy).unwrap_or_default()
            }
        }
    }
}

/// Whether `target` is checksum files only, which the menu offers to verify (RC-020-10): named so,
/// and no folder among them.
fn checksum_files(pane: &Pane, target: &Target) -> bool {
    let Target::Entries(names) = target else {
        return false;
    };
    let named = |name: &String| Algorithm::of_file_name(name).is_some();
    if names.is_empty() || !names.iter().all(named) {
        return false;
    }
    let names: HashSet<&str> = names.iter().map(String::as_str).collect();
    pane.entries()
        .iter()
        .filter(|entry| names.contains(entry.name.as_str()))
        .all(is_checksum_file)
}

/// Whether `path` is on the machine's own file systems, which the Explorer knows: not a server,
/// not the inside of an archive.
fn is_local(path: &VfsPath) -> bool {
    *path == VfsPath::local(path.path.clone())
}

/// The id of a panel's list area, which is measured to place the menu.
pub(super) fn list_id(panel: PanelId) -> Id {
    Id::new(match panel {
        PanelId::Left => "rcommander.list.left",
        PanelId::Right => "rcommander.list.right",
    })
}

/// Finds where the list of `panel` is in the window.
fn measure(panel: PanelId) -> Task<Message> {
    operate(ListBounds {
        id: list_id(panel),
        found: None,
    })
    .map(|list| Message::ContextMenu(ContextMessage::Measured(list)))
}

/// A widget operation that finds the bounds of the container with `id`.
struct ListBounds {
    id: Id,
    found: Option<Rectangle>,
}

impl Operation<Option<Rectangle>> for ListBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<Rectangle>>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.id) {
            self.found = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Option<Rectangle>> {
        Outcome::Some(self.found)
    }
}

#[cfg(test)]
#[path = "context_menu_tests.rs"]
mod tests;
