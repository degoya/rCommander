//! The window's list (RC-100-68): the workspaces in their order, narrowed by the filter, with the
//! cursor kept on a workspace by its id - a new list after a change keeps it where it was; the
//! place a workspace moves to with Ctrl+Up or Ctrl+Down; a name no workspace has yet for a new
//! one.

use rc_engine::WorkspaceId;

use crate::app::palette::workspaces::WorkspaceRow;

/// Rows a page key moves.
pub const PAGE: isize = 10;

/// The workspaces of `rows` whose name holds every word of `filter`, ignoring case, in their
/// order.
pub fn shown(rows: &[WorkspaceRow], filter: &str) -> Vec<WorkspaceId> {
    let filter = filter.to_lowercase();
    let words: Vec<&str> = filter.split_whitespace().collect();
    rows.iter()
        .filter(|row| {
            let name = row.name.to_lowercase();
            words.iter().all(|word| name.contains(word))
        })
        .map(|row| row.id)
        .collect()
}

/// The cursor among `shown`: `cursor` while it is shown, else `active` while that is, else the
/// first; `None` for an empty list.
pub fn keep(
    shown: &[WorkspaceId],
    cursor: Option<WorkspaceId>,
    active: Option<WorkspaceId>,
) -> Option<WorkspaceId> {
    cursor
        .filter(|id| shown.contains(id))
        .or_else(|| active.filter(|id| shown.contains(id)))
        .or_else(|| shown.first().copied())
}

/// The cursor moved by `by` rows among `shown`, staying in the list; the first row when the
/// cursor is on none of them.
pub fn step(shown: &[WorkspaceId], cursor: Option<WorkspaceId>, by: isize) -> Option<WorkspaceId> {
    let Some(at) = cursor.and_then(|id| shown.iter().position(|shown| *shown == id)) else {
        return shown.first().copied();
    };
    let last = shown.len().saturating_sub(1);
    shown.get(at.saturating_add_signed(by).min(last)).copied()
}

/// The place among all of `rows` (0-based) that `id` takes one up (`up`) or one down; `None` at
/// either end and for a workspace not listed.
pub fn moved_to(rows: &[WorkspaceRow], id: WorkspaceId, up: bool) -> Option<usize> {
    let at = rows.iter().position(|row| row.id == id)?;
    if up {
        at.checked_sub(1)
    } else {
        (at + 1 < rows.len()).then_some(at + 1)
    }
}

/// A name for a new workspace that no workspace of `rows` has, ignoring case: `base` with the
/// first free number, "Workspace 1", "Workspace 2", ...
pub fn new_name(rows: &[WorkspaceRow], base: &str) -> String {
    (1..=rows.len() + 1)
        .map(|number| format!("{base} {number}"))
        .find(|name| {
            !rows
                .iter()
                .any(|row| row.name.to_lowercase() == name.to_lowercase())
        })
        .unwrap_or_else(|| base.to_owned())
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
