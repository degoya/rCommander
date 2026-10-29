//! The `panels` tool group: thin wrappers over the `panel.*` commands.

use rc_engine::{
    Command, CursorTarget, Entry, EntryKind, Outcome, Owner, PanelId, PanelSnapshot, VaultStatus,
    VfsPath,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result},
};

/// A panel, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PanelArg {
    Left,
    Right,
}

impl From<PanelArg> for PanelId {
    fn from(panel: PanelArg) -> Self {
        match panel {
            PanelArg::Left => Self::Left,
            PanelArg::Right => Self::Right,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListDirParams {
    /// The directory: absolute, or relative to the panel's directory (`..` goes up). Omitted:
    /// the panel's own directory.
    #[serde(default)]
    pub path: Option<String>,
    /// The panel a relative path starts from; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct PanelParams {
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct NavigateParams {
    /// The directory to show: absolute, or relative to the panel's directory (`..` goes up).
    pub path: String,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SetCursorParams {
    /// The entry's position in the panel's order (directories first, then by its sort key).
    #[serde(default)]
    pub index: Option<usize>,
    /// The entry's name; give either this or `index`.
    #[serde(default)]
    pub name: Option<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ToggleSelectionParams {
    /// Entry names to select, or deselect when already selected.
    pub names: Vec<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// A listing as tools answer it: compact, times as Unix seconds.
#[derive(Serialize)]
struct ListingView {
    path: VfsPath,
    entries: Vec<EntryView>,
}

#[derive(Serialize)]
pub(crate) struct EntryView {
    name: String,
    kind: &'static str,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_unix: Option<u64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    hidden: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    readonly: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    link_target: Option<String>,
    /// User and group, where the backend names them (SFTP, local Unix; RC-060-02).
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<Owner>,
}

impl From<Entry> for EntryView {
    fn from(entry: Entry) -> Self {
        let modified_unix = entry.modified_unix();
        let (kind, link_target) = match entry.kind {
            EntryKind::File => ("file", None),
            EntryKind::Dir => ("dir", None),
            EntryKind::Symlink { target, .. } => {
                ("symlink", Some(target.to_string_lossy().into_owned()))
            }
            EntryKind::Other => ("other", None),
        };
        Self {
            name: entry.name,
            kind,
            size: entry.size,
            modified_unix,
            hidden: entry.hidden,
            readonly: entry.attributes.readonly,
            link_target,
            owner: entry.owner,
        }
    }
}

/// An outcome as tools answer it.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    Listing(ListingView),
    Panel(PanelSnapshot),
    /// What the panel commands never answer; passed through as the engine serializes it.
    Other(Box<Outcome>),
    Vault(VaultStatus),
}

fn view(outcome: Outcome) -> View {
    match outcome {
        Outcome::Listing { path, entries } => View::Listing(ListingView {
            path,
            entries: entries.into_iter().map(EntryView::from).collect(),
        }),
        Outcome::Panel(snapshot) => View::Panel(snapshot),
        Outcome::Vault(status) => View::Vault(status),
        // The panel commands answer nothing else.
        other => View::Other(Box::new(other)),
    }
}

impl RcMcpServer {
    pub(crate) async fn run(&self, command: Command) -> ToolResult {
        self.dispatch_as(command, view).await
    }
}

#[tool_router(router = panels_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List a directory's entries (name, kind file/dir/symlink/other, size, modified_unix, hidden, readonly, link_target, owner with user and group where the backend names them: SFTP, local Unix) in the panel's order (directories first, then by its sort key), hidden entries included. Moves no panel. path is absolute or relative to the panel's directory; omitted, the panel's own directory. Errors: not_found, not_a_directory, permission_denied, ..."
    )]
    pub(crate) async fn list_dir(
        &self,
        Parameters(params): Parameters<ListDirParams>,
    ) -> ToolResult {
        self.run(Command::ListDir {
            panel: params.panel.map(Into::into),
            path: params.path,
        })
        .await
    }

    #[tool(
        description = "Read a panel: its path, whether it is active, the cursor (index and name), the selected names, the entry count, its sort, its view mode (full or brief) and whether hidden entries are shown; loading true while a long listing still arrives. A panel showing its terminal (view terminal) also has terminal: the session id, the folder the shell last reported (absent without shell integration), prompt (at_prompt, running, or unknown without shell integration) and follow (why the shell is not in the panel's folder: no_integration, running, line_not_empty, unreachable). The active panel when panel is omitted."
    )]
    pub(crate) async fn get_panel_state(
        &self,
        Parameters(params): Parameters<PanelParams>,
    ) -> ToolResult {
        self.run(Command::GetPanelState {
            panel: params.panel.map(Into::into),
        })
        .await
    }

    #[tool(
        description = "Show another directory in a panel, as the window does: path is absolute, or relative to the panel's directory, and .. goes up (the cursor then stands on the directory just left). An archive file, or a path through one (C:\\data\\backup.zip\\docs), opens the archive there, read-only; inside it /docs starts at the archive's root and .. at its root leaves it. Clears the panel's selection. Answers the panel's new state - with loading true while a long listing (a large server folder) still arrives: entry_count counts what came so far, get_panel_state tells when it is complete; errors not_found, not_a_directory, permission_denied leave the panel where it was."
    )]
    pub(crate) async fn navigate(
        &self,
        Parameters(params): Parameters<NavigateParams>,
    ) -> ToolResult {
        self.run(Command::Navigate {
            panel: params.panel.map(Into::into),
            path: params.path,
        })
        .await
    }

    #[tool(
        description = "Put a panel's cursor on an entry, by index (panel order) or by name - one of the two. Errors: cursor_out_of_range, entry_not_found."
    )]
    pub(crate) async fn set_cursor(
        &self,
        Parameters(params): Parameters<SetCursorParams>,
    ) -> ToolResult {
        let target = match (params.index, params.name) {
            (Some(index), None) => CursorTarget::Index(index),
            (None, Some(name)) => CursorTarget::Name(name),
            _ => {
                return Ok(error_result(
                    "invalid_params",
                    "give either index or name, not both or neither",
                ));
            }
        };
        self.run(Command::SetCursor {
            panel: params.panel.map(Into::into),
            target,
        })
        .await
    }

    #[tool(
        description = "Select the named entries of a panel, deselecting those already selected. One unknown name (entry_not_found) changes nothing."
    )]
    pub(crate) async fn toggle_selection(
        &self,
        Parameters(params): Parameters<ToggleSelectionParams>,
    ) -> ToolResult {
        self.run(Command::ToggleSelection {
            panel: params.panel.map(Into::into),
            names: params.names,
        })
        .await
    }

    #[tool(
        description = "Make a panel the active one (keyboard input goes there); the other panel when panel is omitted. Answers the newly active panel's state."
    )]
    pub(crate) async fn switch_active_panel(
        &self,
        Parameters(params): Parameters<PanelParams>,
    ) -> ToolResult {
        self.run(Command::SwitchActivePanel {
            panel: params.panel.map(Into::into),
        })
        .await
    }

    #[tool(
        description = "Turn a panel that shows a terminal back to its files; the shell keeps running and terminal_show shows it again. The active panel when panel is omitted. Answers the panel's state."
    )]
    pub(crate) async fn show_files(
        &self,
        Parameters(params): Parameters<PanelParams>,
    ) -> ToolResult {
        self.run(Command::ShowFiles {
            panel: params.panel.map(Into::into),
        })
        .await
    }
}

#[cfg(test)]
#[path = "panels_tests.rs"]
mod tests;
