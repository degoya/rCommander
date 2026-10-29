//! The hotlist dialog ("Configure" in the Ctrl+D menu): the hotlist as an indented list, the
//! chosen row's name, folder and other-panel folder to edit, and the buttons that add, move and
//! delete. Pure data: every button becomes an engine command, and the list is read again when the
//! engine announces the change.

use std::{collections::HashMap, fmt};

use rc_engine::{
    AddHotlistFolderRequest, AddHotlistRequest, Command, HotlistEntry, MoveHotlistRequest, PanelId,
    UpdateHotlistRequest,
};

use crate::i18n::t;

/// The open dialog.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HotlistEditor {
    /// The hotlist in menu order.
    pub items: Vec<HotlistEntry>,
    /// How deep each item is, by its index.
    pub depths: Vec<usize>,
    /// The chosen row, by its id.
    pub chosen: Option<i64>,
    pub name: String,
    /// The folder, as typed: absolute, or relative to the active panel's.
    pub path: String,
    /// The other panel's folder; empty for none.
    pub target: String,
}

/// A place a row can move to: the top or a hotlist folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderChoice {
    pub id: Option<i64>,
    pub label: String,
}

impl fmt::Display for FolderChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// How many hotlist folders each item of `items` (in menu order) is in.
pub fn depths(items: &[HotlistEntry]) -> Vec<usize> {
    let mut by_id: HashMap<i64, usize> = HashMap::new();
    items
        .iter()
        .map(|item| {
            let depth = item
                .parent_id
                .and_then(|parent| by_id.get(&parent))
                .map_or(0, |depth| depth + 1);
            by_id.insert(item.id, depth);
            depth
        })
        .collect()
}

impl HotlistEditor {
    /// The dialog over `items`, nothing chosen.
    #[cfg(test)]
    pub fn new(items: Vec<HotlistEntry>) -> Self {
        let mut editor = Self::default();
        editor.reload(items);
        editor
    }

    /// The list read again; the chosen row stays chosen while it is there, its fields as typed.
    pub fn reload(&mut self, items: Vec<HotlistEntry>) {
        self.depths = depths(&items);
        self.items = items;
        if self.chosen_item().is_none() {
            self.chosen = None;
        }
    }

    /// The chosen row.
    pub fn chosen_item(&self) -> Option<&HotlistEntry> {
        let id = self.chosen?;
        self.items.iter().find(|item| item.id == id)
    }

    /// Chooses row `index` and fills the fields with it.
    pub fn choose(&mut self, index: usize) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        self.chosen = Some(item.id);
        self.name.clone_from(&item.name);
        self.path = display(item.path.as_ref());
        self.target = display(item.target.as_ref());
    }

    /// Stores the fields into the chosen row: a hotlist folder only takes its name, an empty
    /// target removes the target.
    pub fn save(&self) -> Option<Command> {
        let item = self.chosen_item()?;
        let entry = !item.folder;
        let path = (entry && !self.path.trim().is_empty()).then(|| self.path.trim().to_owned());
        let target = self.target.trim();
        Some(Command::UpdateHotlist(UpdateHotlistRequest {
            id: item.id,
            panel: None,
            name: Some(self.name.clone()),
            path,
            target: (entry && !target.is_empty()).then(|| target.to_owned()),
            clear_target: entry && target.is_empty() && item.target.is_some(),
        }))
    }

    /// Adds `panel`'s folder: into the chosen hotlist folder, or beside the chosen entry.
    pub fn add_current(&self, panel: PanelId) -> Command {
        Command::AddHotlist(AddHotlistRequest {
            panel: Some(panel),
            parent_id: self.parent_for_new(),
            ..AddHotlistRequest::default()
        })
    }

    /// A new hotlist folder, "New folder" until it is renamed, where
    /// [`HotlistEditor::add_current`] adds.
    pub fn add_folder(&self) -> Command {
        Command::AddHotlistFolder(AddHotlistFolderRequest {
            name: t!("hotlist.new_folder_name").to_owned(),
            parent_id: self.parent_for_new(),
        })
    }

    /// Moves the chosen row one place up (`-1`) or down (`1`) among its siblings.
    pub fn step(&self, by: isize) -> Option<Command> {
        let item = self.chosen_item()?;
        let siblings: Vec<i64> = self
            .items
            .iter()
            .filter(|other| other.parent_id == item.parent_id)
            .map(|other| other.id)
            .collect();
        let at = siblings.iter().position(|id| *id == item.id)?;
        let to = at
            .checked_add_signed(by)
            .filter(|to| *to < siblings.len())?;
        Some(Command::MoveHotlist(MoveHotlistRequest {
            id: item.id,
            parent_id: item.parent_id,
            position: Some(to),
        }))
    }

    /// Moves the chosen row into `folder` (the top when `None`), after its last row.
    pub fn move_to(&self, folder: Option<i64>) -> Option<Command> {
        let item = self.chosen_item()?;
        (item.parent_id != folder).then_some(Command::MoveHotlist(MoveHotlistRequest {
            id: item.id,
            parent_id: folder,
            position: None,
        }))
    }

    /// Deletes the chosen row, a hotlist folder with what it holds.
    pub fn delete(&self) -> Option<Command> {
        Some(Command::DeleteHotlist {
            id: self.chosen_item()?.id,
        })
    }

    /// Where the chosen row can move: the top and every hotlist folder outside it.
    pub fn folders(&self) -> Vec<FolderChoice> {
        let mut choices = vec![FolderChoice {
            id: None,
            label: t!("hotlist.top").to_owned(),
        }];
        let inside = self.chosen_subtree();
        for (item, depth) in self.items.iter().zip(&self.depths) {
            if item.folder && !inside.contains(&item.id) {
                choices.push(FolderChoice {
                    id: Some(item.id),
                    label: format!("{}{}", "  ".repeat(*depth), item.name),
                });
            }
        }
        choices
    }

    /// The chosen row's parent as a choice of [`HotlistEditor::folders`].
    pub fn chosen_folder(&self) -> Option<FolderChoice> {
        let parent = self.chosen_item()?.parent_id;
        self.folders()
            .into_iter()
            .find(|choice| choice.id == parent)
    }

    /// The hotlist folder a new row goes into.
    fn parent_for_new(&self) -> Option<i64> {
        let item = self.chosen_item()?;
        if item.folder {
            Some(item.id)
        } else {
            item.parent_id
        }
    }

    /// The chosen row and every row inside it (menu order keeps a folder's rows behind it).
    fn chosen_subtree(&self) -> Vec<i64> {
        let Some(start) = self
            .chosen
            .and_then(|id| self.items.iter().position(|item| item.id == id))
        else {
            return Vec::new();
        };
        let depth = self.depths[start];
        let mut ids = vec![self.items[start].id];
        ids.extend(
            self.items[start + 1..]
                .iter()
                .zip(&self.depths[start + 1..])
                .take_while(|(_, below)| **below > depth)
                .map(|(item, _)| item.id),
        );
        ids
    }
}

/// A folder as the fields show it.
fn display(path: Option<&rc_engine::VfsPath>) -> String {
    path.map(|path| path.path.display().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;
