//! The Shortcuts tab's "Add key for a workspace..." (RC-100-68), as "Add key for a theme..."
//! (`theme.rs`): a panel under the table chooses a workspace - "Workspace: Servers", as the
//! command palette names it -, then "Record key" takes the next key pressed as the recording of
//! "Add key" does, the question before a taken key moves included. The key becomes a row of its
//! own, `workspace:<id>` (`palette::workspaces::key_name`), which the table shows, changes,
//! removes and resets like any other and OK or Apply store in `keys.overrides`. The workspaces
//! window's "Add key..." opens the panel with its workspace chosen.
//!
//! Keys while the panel is open: Enter records, Escape closes it, the arrows and Delete leave the
//! table alone; Tab walks the panel's controls.

use std::fmt;

use super::{Recording, Selected};
use crate::app::{
    App, Effect,
    palette::workspaces::{WorkspaceRow, key_name, row_label, rows},
};

/// The open panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspacePick {
    /// The workspace key's name of the workspace chosen; none before one is.
    pub name: Option<String>,
}

/// A workspace in the panel's list: its key's name and its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOption {
    pub name: String,
    pub label: String,
}

impl fmt::Display for WorkspaceOption {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

#[derive(Debug, Clone)]
pub enum WorkspaceKeyMsg {
    /// "Add key for a workspace..." pressed.
    Open,
    /// The workspaces window's "Add key...": the panel with this workspace key chosen.
    OpenFor(String),
    Pick(WorkspaceOption),
    /// "Record key": the next key pressed is the workspace's.
    Record,
    Cancel,
}

/// The panel's list: every workspace of `rows`, in their order, as the palette names them.
pub fn options(rows: &[WorkspaceRow]) -> Vec<WorkspaceOption> {
    rows.iter()
        .map(|row| WorkspaceOption {
            name: key_name(row.id),
            label: row_label(&row.name),
        })
        .collect()
}

/// The workspace the panel offers first: the active one, else the first.
fn first(rows: &[WorkspaceRow]) -> Option<String> {
    rows.iter()
        .find(|row| row.active)
        .or_else(|| rows.first())
        .map(|row| key_name(row.id))
}

impl App {
    /// A message of the panel.
    pub(in crate::app::settings) fn workspace_key_message(
        &mut self,
        message: WorkspaceKeyMsg,
    ) -> Vec<Effect> {
        let Some(dialog) = self.settings.as_mut() else {
            return Vec::new();
        };
        let page = &mut dialog.shortcuts;
        let idle = page.recording.is_none() && page.asking.is_none();
        match message {
            WorkspaceKeyMsg::Open | WorkspaceKeyMsg::OpenFor(_) if !idle => {}
            WorkspaceKeyMsg::Open => {
                page.adding = None;
                page.adding_workspace = Some(WorkspacePick {
                    name: first(&rows()),
                });
                dialog.note = None;
            }
            WorkspaceKeyMsg::OpenFor(name) => {
                page.adding = None;
                page.adding_workspace = Some(WorkspacePick { name: Some(name) });
                dialog.note = None;
            }
            WorkspaceKeyMsg::Pick(option) => {
                if let Some(adding) = page.adding_workspace.as_mut() {
                    adding.name = Some(option.name);
                }
            }
            WorkspaceKeyMsg::Record => {
                if let Some(name) = page.adding_workspace.take().and_then(|adding| adding.name) {
                    page.selected = Some(Selected {
                        name,
                        keys: String::new(),
                    });
                    page.recording = Some(Recording::Add);
                }
            }
            WorkspaceKeyMsg::Cancel => page.adding_workspace = None,
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
