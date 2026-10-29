//! The `agent` tool group: which keys the running SSH agents hold.
//!
//! The answer names the agent, key type, comment and SHA256 fingerprint of each key - never a
//! public or private key blob. No tool adds, removes or uses an agent key: a key is used only
//! inside an SSH login rCommander makes (RC-060-01), and nothing here needs the vault.

use rc_engine::Command;
use rmcp::{tool, tool_router};

use crate::{RcMcpServer, answer::ToolResult};

#[tool_router(router = agent_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the keys the running SSH agents hold, for connections with auth_method agent: sources (the agents that answered: ssh_auth_sock on Linux and macOS, openssh_pipe for the Windows OpenSSH agent service, pageant) and identities, each with source, algorithm, fingerprint_sha256 as ssh-add -l prints it, and comment. Never key bytes. Answers within about a second. Errors: agent_not_found (no agent runs; a plain answer), agent_timeout, agent_failed."
    )]
    pub(crate) async fn ssh_agent_identities(&self) -> ToolResult {
        self.dispatch_mcp(Command::ListAgentIdentities).await
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
