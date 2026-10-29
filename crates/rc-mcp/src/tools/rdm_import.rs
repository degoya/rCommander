//! `import_remote_desktop_manager`: the thin wrapper over `import.remote_desktop_manager`
//! (RC-050-11).
//!
//! A write tool, as the command is registered, its preview (`dry_run`, the default) included, as
//! the other imports are. It reads only a file ending in `.rdm` that the read guard lets through
//! under its own reason (`rc_engine::RDM_EXPORT_PASSWORDS`; the reading tools refuse it),
//! and never a password: the answer says per entry only whether RDM keeps one
//! (`password_state`), and a connection is always created without one.

use rc_engine::{Command, Outcome, RdmImportRequest, import::rdm_report::RdmImportReport};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ImportRdmParams {
    /// Path of the export: a .rdm file Remote Desktop Manager wrote without a master key.
    pub path: String,
    /// true (the default): only the preview, nothing changes. false imports.
    #[serde(default)]
    pub dry_run: Option<bool>,
    /// The keys of the entries to import, from a preview (RDM's IDs in lower case); omit it for
    /// everything new or changed.
    #[serde(default)]
    pub only: Option<Vec<String>>,
}

/// The report itself, without the outcome's `kind`.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    Report(Box<RdmImportReport>),
    Other(Box<Outcome>),
}

#[tool_router(router = rdm_import_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Import the SSH connections of a Remote Desktop Manager export (.rdm, written without a master key) into the connection tree: SSH shells, PuTTY SSH, SFTP and SCP entries. dry_run defaults to true: the answer is the preview and nothing changes; call again with dry_run=false to import. A write tool, the preview included. Answers path, dry_run, entries (key: RDM's ID, name, folders, rdm_type, kind, host, port, user, key_path, key_path_refused, password_state: none or obfuscated, selected, status: new, created, update, updated, unchanged or refused with a code - rdm_not_supported_yet for FTP and FTPS, rdm_not_supported for RDP, VNC, web and other types, rdm_no_id, rdm_duplicate_id, rdm_too_deep, rdm_folder_invalid, rdm_value_too_long, or a connection-tree code) and warnings (too_many_entries). Everything goes into the folder Imported from Remote Desktop Manager, RDM's folders inside. An entry imported before is updated by its ID, never added twice; one with a stored secret is not updated (refused, endpoint_change_needs_gui). Passwords are never read or returned. Only a file ending in .rdm is read; the reading tools refuse such files. Errors: rdm_path_refused, rdm_unreadable, rdm_too_large, rdm_encrypted (exported with a master key), rdm_invalid, rdm_not_an_export."
    )]
    pub(crate) async fn import_remote_desktop_manager(
        &self,
        Parameters(params): Parameters<ImportRdmParams>,
    ) -> ToolResult {
        let command = Command::ImportRemoteDesktopManager(RdmImportRequest {
            path: params.path,
            dry_run: params.dry_run.unwrap_or(true),
            only: params.only,
        });
        self.dispatch_as(command, |outcome| match outcome {
            Outcome::RdmImported(report) => View::Report(Box::new(report)),
            other => View::Other(Box::new(other)),
        })
        .await
    }
}

#[cfg(test)]
#[path = "rdm_import_tests.rs"]
mod tests;
