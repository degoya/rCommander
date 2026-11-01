//! The workspaces in the command palette and the menu bar (RC-100-68): one row per workspace,
//! "Workspace: Servers", made from the workspaces the window knows (`workspaces.list`, read at
//! the start and again after every change, `app/workspaces/known.rs`) each time the palette
//! opens; a row runs `workspaces.apply { id }`, and the active workspace's row is marked. A
//! workspace with a key of its own (the Shortcuts tab's "Add key for a workspace...") is that
//! key's row, which shows the key. `reach.json` names the command `listed`.
//!
//! A workspace's key is named `workspace:<id>`, as the engine's keymap reads it
//! (`rc_engine::workspace_key`, `keymap/workspace_keys.rs`); the same words name it wherever the
//! name of a key's row is shown ([`label`]). A key whose workspace was deleted names the id and is
//! [`missing`].
//!
//! The menu bar's "Workspaces" menu lists them above its own entries ([`menu_items`]), the first
//! nine with the digits as their access keys; the system's menu wants ids that live as long as
//! the bar, so each workspace's id is made once a run ([`interned`]).

use std::sync::{Mutex, PoisonError};

use rc_engine::{Command, Keymap, WorkspaceId, workspace_key, workspace_key_name};

use super::{Action, Entry};
use crate::{i18n::t, platform::menu_bar::BarItem};

/// The command whose palette rows a list makes.
#[cfg(test)]
pub const LISTED: &[&str] = &["workspaces.apply"];

/// The menu of the bar whose entries start with the workspaces.
pub const MENU_ID: &str = "workspaces";

/// A workspace as the palette, the menu and the window's list offer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub id: WorkspaceId,
    pub name: String,
    /// The workspace applied last in this window.
    pub active: bool,
}

/// The workspaces the window knows, in their order.
pub fn rows() -> Vec<WorkspaceRow> {
    crate::app::workspaces::known::rows()
}

/// The name of the key that applies the workspace `id`: `workspace:<id>`, as the engine's keymap
/// names it.
pub fn key_name(id: WorkspaceId) -> String {
    workspace_key_name(id)
}

/// The workspace id the name of a workspace key holds, read by the engine's keymap as untrusted
/// input; `None` for any other name.
pub fn key_id(name: &str) -> Option<WorkspaceId> {
    workspace_key(name)
}

/// "Workspace: Servers".
pub fn row_label(name: &str) -> String {
    t!("palette.workspace", name = name)
}

/// The words of the workspace key `name` with the workspaces the window knows; `None` for the
/// name of any other row.
pub fn label(name: &str) -> Option<String> {
    key_id(name)?;
    label_in(name, &rows())
}

/// The words of the workspace key `name` among `rows`: the workspace's name, or its id when it is
/// none of them; `None` for the name of any other row.
pub fn label_in(name: &str, rows: &[WorkspaceRow]) -> Option<String> {
    let id = key_id(name)?;
    let shown = rows
        .iter()
        .find(|row| row.id == id)
        .map_or_else(|| id.to_string(), |row| row.name.clone());
    Some(row_label(&shown))
}

/// Whether `name` is a workspace key whose workspace is none of `rows`: deleted since the key was
/// added, or of an import from another machine.
pub fn missing(name: &str, rows: &[WorkspaceRow]) -> bool {
    key_id(name).is_some_and(|id| !rows.iter().any(|row| row.id == id))
}

/// `rows` as rows of the palette among `entries`, all sorted by label again: a workspace with a
/// key of its own marks that key's row, every other one is a row of its own that runs
/// `workspaces.apply`.
pub fn add_rows(entries: &mut Vec<Entry>, rows: &[WorkspaceRow]) {
    for row in rows {
        let name = key_name(row.id);
        if let Some(entry) = entries.iter_mut().find(|entry| entry.name == name) {
            entry.label = row_label(&row.name);
            entry.current = row.active;
            continue;
        }
        entries.push(Entry {
            name,
            label: row_label(&row.name),
            keys: String::new(),
            chords: String::new(),
            action: Action::Run(Box::new(Command::ApplyWorkspace { id: row.id })),
            current: row.active,
        });
    }
    entries.sort_by_key(|entry| entry.label.to_lowercase());
}

/// The entries the menu `menu` starts with: for the Workspaces menu each workspace the window
/// knows, then a separator; none for any other menu.
pub fn menu_items(menu: &str, keymap: &Keymap) -> Vec<BarItem> {
    if menu != MENU_ID {
        return Vec::new();
    }
    menu_items_of(&rows(), keymap)
}

/// Each of `rows` as an entry of the menu - its access key a digit for the first nine, its key
/// from `keymap` after a tab - then a separator; nothing without rows.
pub fn menu_items_of(rows: &[WorkspaceRow], keymap: &Keymap) -> Vec<BarItem> {
    let mut items: Vec<BarItem> = rows
        .iter()
        .enumerate()
        .map(|(at, row)| {
            let name = key_name(row.id);
            // A name's own `&` is no access key.
            let words = row.name.replace('&', "&&");
            let label = if at < 9 {
                format!("&{} {words}", at + 1)
            } else {
                words
            };
            let text = match super::menu_bar::key_of(keymap, &name) {
                Some(key) => format!("{label}\t{key}"),
                None => label,
            };
            BarItem::Entry {
                id: interned(name),
                text,
            }
        })
        .collect();
    if !items.is_empty() {
        items.push(BarItem::Separator);
    }
    items
}

/// The ids made for the menu so far, each once.
static IDS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// `name` as an id that lives as long as the bar: the one made before, or a new one kept for the
/// rest of the run - one per workspace seen, so they stay few.
fn interned(name: String) -> &'static str {
    let mut ids = IDS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(id) = ids.iter().copied().find(|id| *id == name) {
        return id;
    }
    let id: &'static str = Box::leak(name.into_boxed_str());
    ids.push(id);
    id
}

#[cfg(test)]
#[path = "workspaces_tests.rs"]
mod tests;
