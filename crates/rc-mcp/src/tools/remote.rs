//! The connections as file systems: `files_stat` reads one entry, `ftp://<connection>/<path>`
//! and `sftp://<connection>/<path>`
//! included, and the FTP options the `connections_*` tools take (RC-070-01). Testing an FTP
//! connection is `connections_test` (RC-060-01), whose answer for FTP is the engine's
//! `connection_tested` outcome. A certificate the system does not trust fails with
//! `certificate_unknown` and its fingerprint; trusting it is the person's decision, never a
//! tool's.

use rc_engine::{Command, FtpDataMode, FtpEncoding, Outcome, VfsPath};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use super::panels::{EntryView, PanelArg};
use crate::{RcMcpServer, answer::ToolResult};

/// An FTP connection's data connection mode, as the connections tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FtpModeArg {
    Passive,
    Active,
}

impl From<FtpModeArg> for FtpDataMode {
    fn from(mode: FtpModeArg) -> Self {
        match mode {
            FtpModeArg::Passive => Self::Passive,
            FtpModeArg::Active => Self::Active,
        }
    }
}

/// An FTP connection's name encoding without UTF-8, as the connections tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FtpEncodingArg {
    Latin1,
    Utf8,
}

impl From<FtpEncodingArg> for FtpEncoding {
    fn from(encoding: FtpEncodingArg) -> Self {
        match encoding {
            FtpEncodingArg::Latin1 => Self::Latin1,
            FtpEncodingArg::Utf8 => Self::Utf8,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct StatParams {
    /// The entry: absolute, relative to the panel's directory, ftp://<connection name>/<path> or
    /// sftp://<connection name>/<path>.
    pub path: String,
    /// The panel a relative path starts from; the active one when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// An outcome as these tools answer it.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    Stat { path: VfsPath, entry: EntryView },
    Other(Box<Outcome>),
}

fn view(outcome: Outcome) -> View {
    match outcome {
        Outcome::Stat { path, entry } => View::Stat {
            path,
            entry: entry.into(),
        },
        other => View::Other(Box::new(other)),
    }
}

#[tool_router(router = remote_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "One entry by its path (name, kind file/dir/symlink/other, size, modified_unix, hidden, readonly, link_target), on this machine, on an FTP connection as ftp://<connection name>/<path> or on an SSH connection over SFTP as sftp://<connection name>/<path> (logs in with what is stored on first use). Errors: not_found, permission_denied, ftp_connection_not_found, ftp_connection_ambiguous, sftp_connection_not_found, sftp_connection_ambiguous, password_not_stored, vault_locked, ftp_* and sftp_* codes, certificate_unknown, host_key_unknown, connection_lost."
    )]
    pub(crate) async fn files_stat(
        &self,
        Parameters(params): Parameters<StatParams>,
    ) -> ToolResult {
        self.dispatch_as(
            Command::Stat {
                panel: params.panel.map(Into::into),
                path: params.path,
            },
            view,
        )
        .await
    }
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
