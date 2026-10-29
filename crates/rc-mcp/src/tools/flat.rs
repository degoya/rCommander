//! The flat view of a panel: `panel_branch_view` (Total Commander's Ctrl+B and Ctrl+Shift+B)
//! and `panel_feed_paths` (the find-files dialog's "Feed to listbox"). Both change what the
//! person sees and answer the panel's state; the files themselves an agent lists with
//! `files_search`. A flat view lists names only, as `list_dir` does, never contents.

use rc_engine::Command;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use super::panels::PanelArg;
use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct BranchViewParams {
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
    /// Only the selected folders and files (or the entry under the cursor), as Ctrl+Shift+B.
    #[serde(default)]
    pub selection: bool,
    /// true (default) starts the flat view; false leaves it into the folder of the entry under
    /// the cursor, the cursor on that entry.
    #[serde(default = "shown")]
    pub show: bool,
}

fn shown() -> bool {
    true
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FeedPathsParams {
    /// Files and folders to show: absolute, or relative to `root`. At most 100000.
    pub paths: Vec<String>,
    /// The folder the panel shows, names relative to it: absolute, or relative to the panel's
    /// folder. The panel's folder when omitted.
    #[serde(default)]
    pub root: Option<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[tool_router(router = flat_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Total Commander's branch view (Ctrl+B) in a panel: every file below the panel's folder in one flat list, each named by its path relative to the folder, folders themselves left out; selection true lists only the selected folders and files (Ctrl+Shift+B). The list fills in the background (flat.loading in the panel state), stops at 100000 files (flat.truncated), follows the panel's sort and hidden-files switch, and the panel's copy, move, delete and open act on the real files. show false leaves the view into the folder of the entry under the cursor. Answers the panel's state; to read the files yourself use files_search with kind files."
    )]
    pub(crate) async fn panel_branch_view(
        &self,
        Parameters(params): Parameters<BranchViewParams>,
    ) -> ToolResult {
        self.run(Command::BranchView {
            panel: params.panel.map(Into::into),
            selection: params.selection,
            show: Some(params.show),
        })
        .await
    }

    #[tool(
        description = "Show given files and folders flat in a panel, as the find-files dialog's Feed to listbox does: the panel shows root (its own folder when omitted) and lists each path once, named relative to root (absolute when outside it); paths missing are left out and counted in flat.skipped. At most 100000 paths (invalid_search beyond). Errors: not_found, not_a_directory for root. Answers the panel's state."
    )]
    pub(crate) async fn panel_feed_paths(
        &self,
        Parameters(params): Parameters<FeedPathsParams>,
    ) -> ToolResult {
        self.run(Command::FeedPaths {
            panel: params.panel.map(Into::into),
            root: params.root,
            paths: params.paths,
        })
        .await
    }
}

#[cfg(test)]
#[path = "flat_tests.rs"]
mod tests;
