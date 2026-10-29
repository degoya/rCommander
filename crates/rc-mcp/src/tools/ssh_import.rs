//! `connections_import_ssh_config`: the thin wrapper over `connections.import_ssh_config`.
//!
//! In the `connections_` group, so the gate refuses a secret among its arguments like the
//! group's other tools. It takes no free path: the engine reads only `~/.ssh/config` and
//! `~/.ssh/config.d/*` (canonical, the user's or a WSL distribution's) and refuses key material.
//! The answer holds host names, users, ports and key file paths - never a key, and of the
//! rest of the file only keywords and line numbers.

use rc_engine::{Command, FolderId, Outcome, SshConfigReport};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ImportSshConfigParams {
    /// Omit it for the user's `~/.ssh/config`. Otherwise only that file, a file in
    /// `~/.ssh/config.d/`, or the same of a WSL distribution's home
    /// (`\\wsl.localhost\<distro>\home\<user>\.ssh\config`); anything else is refused.
    #[serde(default)]
    pub path: Option<String>,
    /// true (the default): only the preview, nothing is written. false imports.
    #[serde(default)]
    pub dry_run: Option<bool>,
    /// The host aliases to import, ignoring case; every one when omitted.
    #[serde(default)]
    pub hosts: Option<Vec<String>>,
    /// The folder id for new connections; the root's "Imported from ssh config" when omitted.
    #[serde(default)]
    pub folder_id: Option<i64>,
}

/// The report itself, without the outcome's `kind`.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    Report(SshConfigReport),
    Other(Box<Outcome>),
}

#[tool_router(router = ssh_import_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Import the hosts of an OpenSSH client config into the connection tree. dry_run defaults to true: the answer is the preview and nothing changes; call again with dry_run=false to import. Answers path, dry_run, folder_id, folder_created, hosts (alias, host, port, user, identity_files as paths, proxy_jump, ignored_options as keywords, file, line, selected, status: new, created, update, updated, unchanged or refused with a code), skipped (pattern or negated Host names) and warnings (code, file, line, detail). A host imported before is updated, never added twice; its name, folder, auth method and notes stay; one with a stored secret is not updated (refused, code endpoint_change_needs_gui). Keys stay paths and are never read. Reads only ~/.ssh/config and ~/.ssh/config.d/* (by canonical path, also a WSL distribution's); a key file, or a file holding a key, stops the import. Warnings name keywords and lines, never text of the file. Errors: ssh_config_path_refused, ssh_config_key_material, ssh_config_unreadable, ssh_config_host_unknown, folder_not_found."
    )]
    pub(crate) async fn connections_import_ssh_config(
        &self,
        Parameters(params): Parameters<ImportSshConfigParams>,
    ) -> ToolResult {
        let command = Command::ImportSshConfig(rc_engine::SshConfigImportRequest {
            path: params.path,
            dry_run: params.dry_run.unwrap_or(true),
            hosts: params.hosts,
            folder_id: params.folder_id.map(FolderId),
        });
        self.dispatch_as(command, |outcome| match outcome {
            Outcome::SshConfigImported(report) => View::Report(report),
            other => View::Other(Box::new(other)),
        })
        .await
    }
}

#[cfg(test)]
#[path = "ssh_import_tests.rs"]
mod tests;
