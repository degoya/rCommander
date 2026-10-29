//! The `hotlist` tool group: the directory hotlist (`hotlist.*`, Ctrl+D), RC-010-03. Entries are
//! folders a panel opens, hotlist folders group them; ids come from hotlist_list.

use rc_engine::{
    AddHotlistFolderRequest, AddHotlistRequest, Command, MoveHotlistRequest, OpenHotlistRequest,
    UpdateHotlistRequest,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult, tools::panels::PanelArg};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct HotlistAddParams {
    /// The folder: absolute, or relative to the panel's folder; the panel's own when omitted.
    #[serde(default)]
    pub path: Option<String>,
    /// The name shown; the folder's last part when omitted.
    #[serde(default)]
    pub name: Option<String>,
    /// A folder the other panel shows with it: absolute, or relative to the other panel's folder.
    #[serde(default)]
    pub target: Option<String>,
    /// The hotlist folder to add it to (an id from hotlist_list); the top when omitted.
    #[serde(default)]
    pub parent_id: Option<i64>,
    /// `left` or `right`, the panel relative paths start from; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct HotlistFolderParams {
    /// The folder's name.
    pub name: String,
    /// The hotlist folder to add it to; the top when omitted.
    #[serde(default)]
    pub parent_id: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct HotlistUpdateParams {
    /// The entry's or hotlist folder's id.
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    /// A new folder, absolute or relative to the panel's folder (entries only).
    #[serde(default)]
    pub path: Option<String>,
    /// A new folder for the other panel, absolute or relative to the other panel's folder.
    #[serde(default)]
    pub target: Option<String>,
    /// Remove the target.
    #[serde(default)]
    pub clear_target: bool,
    /// `left` or `right`, the panel relative paths start from; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct HotlistMoveParams {
    pub id: i64,
    /// The hotlist folder to move it into; the top when omitted.
    #[serde(default)]
    pub parent_id: Option<i64>,
    /// Its place there, from 0; after the last row when omitted.
    #[serde(default)]
    pub position: Option<usize>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct HotlistIdParams {
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct HotlistOpenParams {
    /// An entry's id from hotlist_list.
    pub id: i64,
    /// `left` or `right`, the panel that shows the entry's folder; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[tool_router(router = hotlist_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the directory hotlist in menu order, each hotlist folder before what it holds: id, parent_id, name, folder (a group, no path), path and target (the other panel's folder, or null)."
    )]
    pub(crate) async fn hotlist_list(&self) -> ToolResult {
        self.run(Command::ListHotlist {}).await
    }

    #[tool(
        description = "Add a folder to the hotlist, as Ctrl+D's \"add current folder\": by default the panel's own folder, named after its last part; target is a folder the other panel shows with it. Answers the new entry. Errors: hotlist_not_found, hotlist_not_a_folder (parent_id is an entry), hotlist_too_deep, hotlist_bad_name, hotlist_bad_path."
    )]
    pub(crate) async fn hotlist_add(
        &self,
        Parameters(params): Parameters<HotlistAddParams>,
    ) -> ToolResult {
        self.run(Command::AddHotlist(AddHotlistRequest {
            panel: params.panel.map(Into::into),
            name: params.name,
            path: params.path,
            target: params.target,
            parent_id: params.parent_id,
        }))
        .await
    }

    #[tool(
        description = "Add a hotlist folder (a submenu) to group entries. Answers the new folder. Errors: hotlist_not_found, hotlist_not_a_folder, hotlist_too_deep, hotlist_bad_name."
    )]
    pub(crate) async fn hotlist_add_folder(
        &self,
        Parameters(params): Parameters<HotlistFolderParams>,
    ) -> ToolResult {
        self.run(Command::AddHotlistFolder(AddHotlistFolderRequest {
            name: params.name,
            parent_id: params.parent_id,
        }))
        .await
    }

    #[tool(
        description = "Change a hotlist entry's name, path or target (clear_target removes it), or a hotlist folder's name. Answers the changed row. Errors: hotlist_not_found, hotlist_not_an_entry (a path or target for a folder), hotlist_bad_name, hotlist_bad_path."
    )]
    pub(crate) async fn hotlist_update(
        &self,
        Parameters(params): Parameters<HotlistUpdateParams>,
    ) -> ToolResult {
        self.run(Command::UpdateHotlist(UpdateHotlistRequest {
            id: params.id,
            panel: params.panel.map(Into::into),
            name: params.name,
            path: params.path,
            target: params.target,
            clear_target: params.clear_target,
        }))
        .await
    }

    #[tool(
        description = "Move a hotlist entry or folder to another place, or into another hotlist folder; the rows there are numbered anew. Errors: hotlist_not_found, hotlist_not_a_folder, hotlist_cycle (a folder into itself or below), hotlist_too_deep."
    )]
    pub(crate) async fn hotlist_move(
        &self,
        Parameters(params): Parameters<HotlistMoveParams>,
    ) -> ToolResult {
        self.run(Command::MoveHotlist(MoveHotlistRequest {
            id: params.id,
            parent_id: params.parent_id,
            position: params.position,
        }))
        .await
    }

    #[tool(
        description = "Remove a hotlist entry, or a hotlist folder with everything in it. Only the hotlist changes, never a file. Answers how many rows went. Errors: hotlist_not_found."
    )]
    pub(crate) async fn hotlist_delete(
        &self,
        Parameters(params): Parameters<HotlistIdParams>,
    ) -> ToolResult {
        self.run(Command::DeleteHotlist { id: params.id }).await
    }

    #[tool(
        description = "Open a hotlist entry: the panel shows its folder and, when it has a target, the other panel shows the target. Answers the panel's new state. Errors: hotlist_not_found, hotlist_not_an_entry (a hotlist folder), not_found."
    )]
    pub(crate) async fn hotlist_open(
        &self,
        Parameters(params): Parameters<HotlistOpenParams>,
    ) -> ToolResult {
        self.run(Command::OpenHotlist(OpenHotlistRequest {
            id: params.id,
            panel: params.panel.map(Into::into),
        }))
        .await
    }
}

#[cfg(test)]
#[path = "hotlist_tests.rs"]
mod tests;
