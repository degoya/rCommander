//! The `wsl` tool group: thin wrappers over the `wsl.*` commands.

use rc_engine::Command;
use rmcp::{tool, tool_router};

use crate::{RcMcpServer, answer::ToolResult};

#[tool_router(router = wsl_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the installed WSL distributions: name, is_default, version (1 or 2, null when unknown) and root, the distribution's / as a Windows share path (\\\\wsl.localhost\\<name>\\) that navigate and list_dir accept. Read once and kept; wsl_refresh reads again. Empty without WSL, with WSL switched off, outside Windows, or when wsl.exe does not answer within 3 seconds."
    )]
    pub(crate) async fn wsl_list_distros(&self) -> ToolResult {
        self.dispatch_mcp(Command::ListWslDistros).await
    }

    #[tool(
        description = "Read the installed WSL distributions again (after installing or removing one) and answer the new list, as wsl_list_distros does."
    )]
    pub(crate) async fn wsl_refresh(&self) -> ToolResult {
        self.dispatch_mcp(Command::RefreshWslDistros).await
    }
}
