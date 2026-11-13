//! The workspaces the window knows (RC-100-68): `workspaces.list`'s answer, read at the start and
//! again after every `WorkspacesChanged`, kept for the run as the colour themes are
//! (`theme::themes`), so the command palette, the menu bar, the Shortcuts tab and the window's
//! list read the same list without asking the engine each time they draw.
//!
//! The active workspace's themes go to the window's look with every new list
//! ([`active_themes`], `theme::themes::set_workspace_themes`): they come before the window's
//! theme of their mode while it is active.

use rc_engine::{Workspace, WorkspaceId, settings::themes::ThemeMode, workspaces::theme_of};

use crate::app::{known::Known, palette::workspaces::WorkspaceRow};

static KNOWN: Known<Vec<Workspace>> = Known::new("known.known", Vec::new());
/// The workspace `workspaces.on_start` names, read with the list.
static START: Known<Option<WorkspaceId>> = Known::new("known.start", None);

/// The workspaces from now on.
pub fn set(list: Vec<Workspace>) {
    KNOWN.set(list);
}

/// The workspace called up at the start from now on (`workspaces.on_start`), `None` for none.
pub fn set_start(id: Option<WorkspaceId>) {
    START.set(id);
}

/// The workspace called up at the start, as last read or set.
pub fn start() -> Option<WorkspaceId> {
    START.get()
}

/// Every workspace, in the list's order.
pub fn all() -> Vec<Workspace> {
    KNOWN.get()
}

/// The workspaces as rows of the palette, the menu and the list.
pub fn rows() -> Vec<WorkspaceRow> {
    KNOWN.with(|list| rows_of(list.as_slice()))
}

/// `list` as rows.
pub fn rows_of(list: &[Workspace]) -> Vec<WorkspaceRow> {
    list.iter()
        .map(|workspace| WorkspaceRow {
            id: workspace.id,
            name: workspace.name.clone(),
            active: workspace.active,
        })
        .collect()
}

/// The light and the dark theme of the active workspace of `list`, by id, as the engine reads
/// them (`rc_engine::workspaces::theme_of`); `None` for a mode it has none for, and for both
/// while none is active.
pub fn active_themes(list: &[Workspace]) -> [Option<String>; 2] {
    [ThemeMode::Light, ThemeMode::Dark].map(|mode| theme_of(list, mode).map(str::to_owned))
}

#[cfg(test)]
#[path = "known_tests.rs"]
mod tests;
