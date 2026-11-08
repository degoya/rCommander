//! The `app` tool group: which build of rCommander answers (RC-010-19) and where it keeps its
//! data (RC-100-95).

use rc_engine::Command;
use rmcp::{tool, tool_router};

use crate::{RcMcpServer, answer::ToolResult};

#[tool_router(router = app_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read which build of rCommander answers, as its About page shows it: name, version, commit (eight characters, -dirty for a changed tree; null when built without git), built (UTC; null when unknown), platform (operating system and architecture), license (SPDX), authors, links (source, handbook, changelog) each with url and published - an address not published yet is not reachable for a reader - data_dir (the folder of the database, the plugins and the backups; null when the platform names none) and portable (true when this copy keeps its data in data beside the program). Use it in a bug report. The dependency licences are not part of it."
    )]
    pub(crate) async fn app_about(&self) -> ToolResult {
        self.run(Command::About {}).await
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
