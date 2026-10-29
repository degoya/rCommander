//! The `vault` tool group: the vault's state and locking it. Unlocking is deliberately not a
//! tool: the master password is typed into rCommander only, never handed to an agent.

use rc_engine::Command;
use rmcp::{tool, tool_router};

use crate::{RcMcpServer, answer::ToolResult};

#[tool_router(router = vault_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read the vault of stored secrets (passwords, private keys): state absent (no secret was ever stored), locked or unlocked, the idle lock in minutes (0 = never) and, while unlocked, locks_in_secs. Never returns key material or secrets. The vault cannot be unlocked over MCP; when a tool answers vault_locked, ask the person to unlock it in rCommander."
    )]
    pub(crate) async fn vault_status(&self) -> ToolResult {
        self.run(Command::GetVaultStatus).await
    }

    #[tool(
        description = "Lock the vault now: its key leaves memory and stored secrets need the master password again, which only the person can type in rCommander. Answers the vault's new status; locking a locked or absent vault changes nothing."
    )]
    pub(crate) async fn vault_lock(&self) -> ToolResult {
        self.run(Command::LockVault).await
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
