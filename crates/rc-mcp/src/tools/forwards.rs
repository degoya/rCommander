//! Local port forwards of SSH connections (RC-110-03): an agent lists them with their state,
//! stops one, a connection's or every one, and deletes one. Saving and starting a forward is the
//! person's (`forwards.save`, `forwards.start`: `CommandClass::GuiOnly`, omitted in
//! `coverage.rs`): a listening port reaches every process and user of this computer, and
//! through it the target.

use rc_engine::{Command, ConnectionId};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListParams {
    /// The SSH connection whose forwards to list, from connections_list; omitted, every running
    /// forward of every connection.
    #[serde(default)]
    pub connection_id: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ForwardParams {
    /// The forward's id, from forwards_list.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct StopAllParams {
    /// Only this connection's forwards; omitted, every running one.
    #[serde(default)]
    pub connection_id: Option<i64>,
}

#[tool_router(router = forwards_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List a connection's local port forwards (ssh -L), or every running one: each with id, connection_id, bind_address and bind_port (the listening address on this computer), target_host and target_port (as the SSH server reaches them), auto_start (starts with the session), name, running, connections (how many run through it now) and label. Starting and adding forwards is the person's, in rCommander. Errors: connection_not_found."
    )]
    pub(crate) async fn forwards_list(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::ListForwards {
            connection_id: params.connection_id.map(ConnectionId),
        })
        .await
    }

    #[tool(
        description = "Stop a running port forward: its listener and every connection through it close at once. Answers the ids stopped (none when it did not run)."
    )]
    pub(crate) async fn forwards_stop(
        &self,
        Parameters(params): Parameters<ForwardParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::StopForward { id: params.id })
            .await
    }

    #[tool(
        description = "Stop every running port forward, or those of one connection. Answers the ids stopped."
    )]
    pub(crate) async fn forwards_stop_all(
        &self,
        Parameters(params): Parameters<StopAllParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::StopAllForwards {
            connection_id: params.connection_id.map(ConnectionId),
        })
        .await
    }

    #[tool(
        description = "Delete a saved port forward of a connection, stopping it first when it runs. Answers its id and whether it ran. Errors: forward_not_found."
    )]
    pub(crate) async fn forwards_delete(
        &self,
        Parameters(params): Parameters<ForwardParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::DeleteForward { id: params.id })
            .await
    }
}

#[cfg(test)]
#[path = "forwards_tests.rs"]
mod tests;
