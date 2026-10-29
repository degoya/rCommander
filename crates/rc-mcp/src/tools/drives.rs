//! The `drives` tool group: the drive bar (`drives.list`), changing a panel's drive
//! (`panel.change_drive`) and disconnecting a network drive (`drives.unmap_network`,
//! destructive). The drive menu (`panel.drive_menu`) is the window's, and so is mapping a network
//! drive (`drives.map_network`): Windows' credential dialog needs a person.

use rc_engine::Command;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult, tools::panels::PanelArg};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct DrivesListParams {
    /// Ask every drive again, network drives included (which may wake a sleeping share), and
    /// read the WSL distributions again. Default false.
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ChangeDriveParams {
    /// The drive: an id from drives_list, a drive letter (c, C:, C:\) or a label such as a WSL
    /// distribution's name.
    pub drive: String,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct UnmapNetworkParams {
    /// The network drive: its letter (Z, Z:, Z:\), as drives_list lists it with kind network.
    pub drive: String,
    /// Disconnect even while files on the drive are open; programs may lose unsaved data.
    /// Default false: open files fail with network_drive_in_use.
    #[serde(default)]
    pub force: bool,
}

#[tool_router(router = drives_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the drive bar: every drive (group drives: id such as C:, label, kind fixed/removable/network/optical/ram/unknown, root, state ready/not_ready/waiting/unknown, volume, space.free and space.total in bytes), then the WSL distributions (group wsl) and the servers (group servers, id server:<connection id>; folder: their folders in the connection tree from the root down; an SSH server has an sftp root and opens with panel_change_drive, an FTP server has no root yet; quick connections of the window have negative ids). A drive that does not answer within 2 seconds is waiting; network drives are only asked with refresh."
    )]
    pub(crate) async fn drives_list(
        &self,
        Parameters(params): Parameters<DrivesListParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::ListDrives {
            refresh: params.refresh,
        })
        .await
    }

    #[tool(
        description = "Show a drive in a panel, as a click in the drive bar does: when the other panel shows that drive and this panel is on another one, the panel goes to the other panel's folder (as in Total Commander; setting drives.follow_other_panel, on by default); else it returns to the directory it last showed on that drive, else the drive's root. Answers the panel's new state. Errors: drive_not_found (no such entry), drive_not_ready (no medium, unreachable, or no answer within 2 seconds; the panel stays), backend_unavailable. A server (id server:<connection id>) opens in the directory last shown there, else its start directory or where its login starts: an SSH server over SFTP, its login failing with the codes of connections_connect; an FTP or FTPS server over FTP, failing with the FTP codes (ftp_login_failed, certificate_unknown, ...), vault_locked or password_not_stored."
    )]
    pub(crate) async fn panel_change_drive(
        &self,
        Parameters(params): Parameters<ChangeDriveParams>,
    ) -> ToolResult {
        self.run(Command::ChangeDrive {
            panel: params.panel.map(Into::into),
            drive: params.drive,
        })
        .await
    }

    #[tool(
        description = "Disconnect a network drive (Windows; kind network in drives_list, remote is its \\\\server\\share) and forget it for the next sign-in. Destructive: the first call answers with a question and a confirmation code; ask the person, and only if they agree call again with confirmation set to the code. Without force, open files on the drive fail with network_drive_in_use - ask the person before calling again with force=true. Answers the drive bar as it is afterwards. Mapping a network drive is for the person in the rCommander window only (Windows asks for the credentials). Errors: drive_not_found, drive_not_network, network_drive_in_use, network_drives_unsupported, network_drive_failed."
    )]
    pub(crate) async fn drives_unmap_network(
        &self,
        Parameters(params): Parameters<UnmapNetworkParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::UnmapNetworkDrive {
            drive: params.drive,
            force: params.force,
        })
        .await
    }
}

// The tests expect a Unix root as the first drive; Windows answers with its letters.
#[cfg(all(test, not(windows)))]
#[path = "drives_tests.rs"]
mod tests;
