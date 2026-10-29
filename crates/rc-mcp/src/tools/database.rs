//! The `database` tool group: `db_backup`, a backup of `rcommander.db` into the backup folder
//! beside it (RC-010-12).
//!
//! The tool takes no path - an unknown argument is refused - so an agent can write a backup
//! nowhere but into [`crate::BACKUP_DIR`], under a generated name. A path a person chose and a
//! restore are `GuiOnly` commands without a tool.

use rc_engine::Command;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackupParams {
    /// The code a first call answered with, when the settings ask for one.
    #[serde(default)]
    #[allow(dead_code, reason = "read by the gate, not by the tool")]
    pub confirmation: Option<String>,
}

#[tool_router(router = database_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Back up rCommander's database (settings, connection tree, trusted host keys, the vault's encrypted secrets) into the backup folder beside it, under a generated name rcommander-YYYYMMDD-HHMMSS.db. Takes no path: backups go nowhere else, and no MCP tool reads that folder or restores a backup - restoring is done by the person in rCommander. The copy is consistent while rCommander runs and is checked before it is kept. Answers the file's path and size. Errors: backup_target_unwritable, backup_unverified, backup_failed, database_unavailable."
    )]
    pub(crate) async fn db_backup(
        &self,
        Parameters(_params): Parameters<BackupParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::BackupDatabase).await
    }
}

#[cfg(test)]
#[path = "database_tests.rs"]
mod tests;
