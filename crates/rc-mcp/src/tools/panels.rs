//! The `panels` tool group: thin wrappers over the `panel.*` commands.
//!
//! `list_dir` answers a folder in pages of at most 1000 entries (`paging.rs`), and a panel's
//! state names at most [`SELECTION_SHOWN`] of its selected names with how many there are
//! (RC-110-61): neither answer grows with the folder.

use std::sync::Arc;

use rc_engine::{
    Command, CursorTarget, Entry, EntryKind, Outcome, Owner, PanelId, PanelSnapshot, VaultStatus,
    VfsPath,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{call_cancel, paging};
use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result, json_result, respond},
};

/// The most selected names a panel's state names; `selected_count` counts them all.
pub(crate) const SELECTION_SHOWN: usize = 1000;

/// A panel, as tools name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, schemars::JsonSchema)]
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
#[serde(deny_unknown_fields)]
pub(crate) struct ListDirParams {
    /// The directory: absolute, or relative to the panel's directory (`..` goes up). Omitted:
    /// the panel's own directory.
    #[serde(default)]
    pub path: Option<String>,
    /// The panel a relative path starts from; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
    /// Entries per page: 1000 when omitted, at most 1000.
    #[serde(default)]
    #[schemars(range(min = 1, max = 1000))]
    pub limit: Option<usize>,
    /// The next_cursor of the previous page, with otherwise the same arguments.
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PanelParams {
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NavigateParams {
    /// The directory to show: absolute, or relative to the panel's directory (`..` goes up).
    pub path: String,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

/// A folder's listing as `list_dir` keeps it between its pages.
pub(crate) struct DirListing {
    path: VfsPath,
    entries: Vec<EntryView>,
}

/// One page of a listing as `list_dir` answers it.
#[derive(Serialize)]
struct ListingPage<'a> {
    path: &'a VfsPath,
    entries: &'a [EntryView],
    /// The folder's entries, on every page.
    total: usize,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
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
    /// A panel's state, its selection cut ([`panel_state`]).
    Panel(Value),
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
        Outcome::Panel(snapshot) => View::Panel(panel_state(snapshot)),
        Outcome::Vault(status) => View::Vault(status),
        // The panel commands answer nothing else.
        other => View::Other(Box::new(other)),
    }
}

/// `snapshot` as tools answer it: `selection` holds the first [`SELECTION_SHOWN`] selected names,
/// `selected_count` counts them all, and `selection_truncated` says there are more.
fn panel_state(mut snapshot: PanelSnapshot) -> Value {
    let selection = std::mem::take(&mut snapshot.selection);
    let mut state = serde_json::to_value(&snapshot).unwrap_or_default();
    if let Value::Object(state) = &mut state {
        let shown: Vec<String> = selection.iter().take(SELECTION_SHOWN).cloned().collect();
        state.insert("selection".to_owned(), Value::from(shown));
        state.insert("selected_count".to_owned(), Value::from(selection.len()));
        if selection.len() > SELECTION_SHOWN {
            state.insert("selection_truncated".to_owned(), Value::Bool(true));
        }
    }
    state
}

impl RcMcpServer {
    pub(crate) async fn run(&self, command: Command) -> ToolResult {
        self.dispatch_as(command, view).await
    }
}

#[tool_router(router = panels_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List a directory's entries (name, kind file/dir/symlink/other, size, modified_unix, hidden, readonly, link_target, owner with user and group where the backend names them: SFTP, local Unix) in the panel's order (directories first, then by its sort key), hidden entries included. Moves no panel. path is absolute or relative to the panel's directory; omitted, the panel's own directory. Answers path, entries, total (the folder's entries) and truncated; at most limit entries (1000 when omitted, at most 1000) per call: with truncated true, call again with next_cursor as cursor and otherwise the same arguments for the next page. Errors: not_found, not_a_directory, permission_denied, invalid_params (a cursor of another listing), ..."
    )]
    pub(crate) async fn list_dir(
        &self,
        Parameters(params): Parameters<ListDirParams>,
    ) -> ToolResult {
        let pages = &self.context.list_pages.dirs;
        let request = paging::print(&("list_dir", params.panel, &params.path));
        let start = match pages.start(request, params.cursor.as_deref()) {
            Ok(start) => start,
            Err(refused) => return Ok(refused),
        };
        let listing = match start.kept.clone() {
            Some(kept) => kept,
            None => {
                let command = Command::ListDir {
                    panel: params.panel.map(Into::into),
                    path: params.path,
                };
                let engine = &self.context.engine;
                match engine
                    .dispatch_mcp_cancellable(command, call_cancel())
                    .await
                {
                    Ok(Outcome::Listing { path, entries }) => Arc::new(DirListing {
                        path,
                        entries: entries.into_iter().map(EntryView::from).collect(),
                    }),
                    other => return respond(other.map(view)),
                }
            }
        };
        let total = listing.entries.len();
        let page = match pages.page(
            request,
            paging::print(&listing.path),
            &start,
            params.limit.unwrap_or(paging::PAGE),
            (&listing, total),
        ) {
            Ok(page) => page,
            Err(refused) => return Ok(refused),
        };
        json_result(&ListingPage {
            path: &listing.path,
            entries: listing.entries.get(page.from..page.to).unwrap_or_default(),
            total,
            truncated: page.truncated(),
            next_cursor: page.next_cursor,
        })
    }

    #[tool(
        description = "Read a panel: its path, whether it is active, the cursor (index and name), the selected names (selection: the first 1000, sorted; selected_count: all of them; selection_truncated: true when there are more), the entry count, its sort, its view mode (full or brief) and whether hidden entries are shown; loading true while a long listing still arrives. A panel showing its terminal (view terminal) also has terminal: the session id, the folder the shell last reported (absent without shell integration), prompt (at_prompt, running, or unknown without shell integration) and follow (why the shell is not in the panel's folder: no_integration, running, line_not_empty, unreachable). The active panel when panel is omitted."
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

#[cfg(test)]
#[path = "panels_page_tests.rs"]
mod page_tests;
