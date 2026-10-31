//! The `update` tool group (RC-100-05): what is known about rCommander's self-update, and a check
//! now. Both read: a check changes no file and sends nothing about the installation. Download and
//! install have no tool - replacing the program is a person's decision after the release notes,
//! and an agent never replaces the program it talks to.

use rc_engine::Command;
use rmcp::{tool, tool_router};

use crate::{RcMcpServer, answer::ToolResult};

#[tool_router(router = update_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read rCommander's self-update: running_version, channel (stable or beta) and whether a person chose it, check_enabled and interval_hours, source (official or a mirror), last_check and last_code (null when the check went through, else a stable code such as update.not_published or update.bad_signature), offer (a newer verified version with released_at, notes as plain text and its artifact) or null, download (none, running with bytes, ready, failed with code), install_kind (portable updates itself; scoop, homebrew, winget, deb or rpm with install_command are updated by that package manager; unknown with install_reason only downloads), last_update from the journal (phase, from_version, target_version, reason), auto_install and what it waits for, and leftover_bytes. Installing is only possible in the window."
    )]
    pub(crate) async fn update_status(&self) -> ToolResult {
        self.run(Command::GetUpdateStatus {}).await
    }

    #[tool(
        description = "Check for a newer rCommander now: fetches the signed update manifest of the channel from the release host (a GET of a public file with the user agent rCommander/<version>, nothing else of this installation), verifies signature, channel and freshness, and answers the same status as update_status. A refused or failed check is no error: its code is last_code. Changes no file and installs nothing; the person decides in the window."
    )]
    pub(crate) async fn update_check(&self) -> ToolResult {
        self.run(Command::CheckForUpdates {}).await
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
