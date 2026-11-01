//! The drive bar's servers and folders (`connections.set_favourite`, RC-050-04, and
//! `connections.set_folder_favourite`, RC-100-63). The connection manager itself
//! (`connections.manager`) is the window's.

use rc_engine::{Command, ConnectionId, FolderId};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SetFavouriteParams {
    /// The connection's id, from connections_list.
    pub id: i64,
    /// true makes it a button of the drive bar, false leaves it to its folder's menu.
    pub favourite: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SetFolderFavouriteParams {
    /// The folder's id, from connections_list.
    pub id: i64,
    /// true makes it a button of the drive bar for its servers, false takes the button away.
    pub favourite: bool,
}

#[tool_router(router = favourites_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Choose whether a connection is a button of its own among the servers of the drive bar (favourite: true in drives_list, which lists every server with its folder). By default the connections of the tree's root level are and those in folders are not; this marks one either way. Answers the drive bar as it is now. Errors: connection_not_found."
    )]
    pub(crate) async fn connections_set_favourite(
        &self,
        Parameters(params): Parameters<SetFavouriteParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::SetFavourite {
            id: ConnectionId(params.id),
            favourite: params.favourite,
        })
        .await
    }

    #[tool(
        description = "Choose whether a folder of the connection tree is a button of the drive bar that opens a menu of all the servers inside it, subfolders included. By default the folders of the tree's root level are and those below are not; this marks one either way (connections_list names the marked ones as bar_folders; drives_list gives each server folder_buttons, the lengths of its folder path that are buttons). A folder without servers shows no button. Answers the drive bar as it is now. Errors: folder_not_found."
    )]
    pub(crate) async fn connections_set_folder_favourite(
        &self,
        Parameters(params): Parameters<SetFolderFavouriteParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::SetFolderFavourite {
            id: FolderId(params.id),
            favourite: params.favourite,
        })
        .await
    }
}

#[cfg(test)]
#[path = "favourites_tests.rs"]
mod tests;
