//! Server inventories (RC-100-39): the sources with their last sync and the installed inventory
//! plugins, and a sync of one source (or its preview). Setting a source up, changing its address
//! or its token, and removing it are the person's (`inventory.source_save`,
//! `inventory.source_delete`: `CommandClass::GuiOnly`, omitted in `coverage.rs`). No answer
//! carries a token: the engine never puts one into an outcome, and a value it decrypted for a
//! sync is on the mask.

use rc_engine::{Command, inventory::InventorySourceId};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct RefreshParams {
    /// The source's id, as inventory_sources lists it.
    pub source: i64,
    /// Only the plan: what would be created, updated and marked; nothing is written.
    #[serde(default)]
    pub preview: bool,
}

#[tool_router(router = inventory_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the server inventories (NetBox, Proxmox, ... through an inventory plugin) whose hosts rCommander keeps as connections: each source's id, name, plugin_id, address, folder_id, last_sync_at, last_outcome (ok or a failure code) and the slots a token is stored for - never a token; and the installed inventory plugins (id, name, version, slots). A source is set up only by the person in rCommander."
    )]
    pub(crate) async fn inventory_sources(&self) -> ToolResult {
        self.dispatch_mcp(Command::ListInventorySources {}).await
    }

    #[tool(
        description = "Sync one inventory source's folder of connections with the hosts its inventory lists now: hosts matched by the inventory's id; name, address, port and folder updated; the person's own fields (credentials, login script) kept; a host that left marked, never deleted. preview=true only plans. Answers counts (created, updated, unchanged, missing, still_missing, returned, skipped, refused) and the first refusals. Errors: inventory_source_not_found, inventory_plugin_missing, vault_locked (the token needs the vault), plugin_inventory_too_large (more than 10 000 hosts), plugin_http_denied, plugin_secret_missing, plugin_failed."
    )]
    pub(crate) async fn inventory_refresh(
        &self,
        Parameters(params): Parameters<RefreshParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::RefreshInventory {
            source: InventorySourceId(params.source),
            preview: params.preview,
        })
        .await
    }
}

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
