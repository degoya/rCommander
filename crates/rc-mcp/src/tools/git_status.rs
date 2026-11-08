//! `panel_git_status`: the git marks and the branch a panel knows of its folder (RC-100-97). It
//! reads what the window shows and starts no process; git runs only where the person switched
//! `panels.git_status` on, when a panel shows a folder anew.

use rc_engine::Command;
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};

use super::panels::PanelParams;
use crate::{RcMcpServer, answer::ToolResult};

#[tool_router(router = git_status_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read the git status a panel knows of its folder, as the file list marks it: git null when the setting panels.git_status is off (the default; only the person switches it on), the folder is in no repository, lies on a server, in an archive or on a device, or git has not answered yet. Otherwise git.marks maps entry names to modified, added, renamed, untracked, conflict or ignored (a folder: its most important change below it), git.whole marks every entry of a folder git names whole, git.deleted counts files deleted from it, git.branch has head (absent when detached), oid, upstream, ahead and behind. Starts no process: after a navigation the marks follow within a second. The active panel when panel is omitted."
    )]
    pub(crate) async fn panel_git_status(
        &self,
        Parameters(params): Parameters<PanelParams>,
    ) -> ToolResult {
        self.run(Command::GitStatus {
            panel: params.panel.map(Into::into),
        })
        .await
    }
}

#[cfg(test)]
#[path = "git_status_tests.rs"]
mod tests;
