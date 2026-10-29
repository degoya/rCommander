//! The drive bar's servers (`connections.set_favourite`, RC-050-04). The connection manager
//! itself (`connections.manager`) is the window's.

use rc_engine::{Command, ConnectionId};
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
}

#[cfg(test)]
#[path = "favourites_tests.rs"]
mod tests;
