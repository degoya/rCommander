//! The server files a person edits through watched local copies (RC-110-01): an agent reads
//! their list - name, server, path, state, the question waiting -, never their content.
//! Uploading, skipping and closing act on the person's editor session and the upload is their
//! decision (`remote_edit.upload`, `.skip`, `.close`: `CommandClass::GuiOnly`, omitted in
//! `coverage.rs`); `file_open` on a server file gives an agent only a read-only copy nobody
//! watches, so no agent sets up a way back to a server.

use rc_engine::Command;
use rmcp::{tool, tool_router};

use crate::{RcMcpServer, answer::ToolResult};

#[tool_router(router = remote_edit_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the server files (SFTP, FTP, FTPS) the person edits in rCommander through local copies, oldest first: each with id, name, server (the connection's name), path (backend and path on the server), state (unchanged, changed, uploading, uploaded with at, conflict, failed with code) and question (upload, server_changed, server_gone, in_place or kept) while one waits for the person. Never the files' content. Uploading is the person's decision, in rCommander."
    )]
    pub(crate) async fn remote_edits_list(&self) -> ToolResult {
        self.dispatch_mcp(Command::ListRemoteEdits {}).await
    }
}

#[cfg(test)]
#[path = "remote_edit_tests.rs"]
mod tests;
