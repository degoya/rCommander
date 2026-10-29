//! Importing connections from other programs' configuration (RC-050-06), Total Commander's
//! hotlist, tabs and FTP connections (RC-050-09) and Remote Desktop Manager's connections
//! (RC-050-11); rCommander's own export and import of its settings and connections (RC-080-19).

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::{
    FolderId, SecretParam,
    exchange::ExportParts,
    import::total_commander_report::{TcParts, TcTabsMode},
};

/// The parameters of `config.export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigExportRequest {
    /// The file to write (`.rcexport`).
    pub path: String,
    #[serde(default)]
    pub parts: ExportParts,
    /// The export password the file is sealed with; unencrypted when `None`.
    #[serde(default)]
    pub password: Option<SecretParam>,
    /// Passwords, private keys and passphrases go with the connections.
    #[serde(default)]
    pub secrets: bool,
    /// The person confirmed that an unencrypted file holds them in clear.
    #[serde(default)]
    pub plain_secrets: bool,
    /// Replace the file if it exists.
    #[serde(default)]
    pub overwrite: bool,
}

/// The parameters of `config.import`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigImportRequest {
    /// The `.rcexport` file.
    pub path: String,
    /// Its export password, when it is sealed.
    #[serde(default)]
    pub password: Option<SecretParam>,
    /// Only the preview: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
    /// The keys of the rows to import; every new or changed row without a warning when `None`.
    #[serde(default)]
    pub only: Option<Vec<String>>,
}

/// The parameters of `connections.import_ssh_config`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfigImportRequest {
    /// `~/.ssh/config` (`%USERPROFILE%\.ssh\config`) when `None`; else only that file or one
    /// in `~/.ssh/config.d/`, the user's or a WSL distribution's (`ssh_config_path`).
    #[serde(default)]
    pub path: Option<String>,
    /// Only the preview: what an import would create or update; nothing is written.
    #[serde(default)]
    pub dry_run: bool,
    /// The host aliases to import, ignoring ASCII case; every one when `None`.
    #[serde(default)]
    pub hosts: Option<Vec<String>>,
    /// The folder for new connections; the root's "Imported from ssh config" when `None`.
    #[serde(default)]
    pub folder_id: Option<FolderId>,
}

/// The parameters of `import.total_commander`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcImportRequest {
    /// `wincmd.ini`; found by itself (`%APPDATA%\GHISLER`, then Total Commander's registry key)
    /// when `None`. Over MCP only a file of that name.
    #[serde(default)]
    pub wincmd_ini: Option<String>,
    /// `wcx_ftp.ini`; where `wincmd.ini` names it or beside it when `None`. Over MCP only a file
    /// of that name.
    #[serde(default)]
    pub ftp_ini: Option<String>,
    #[serde(default)]
    pub parts: TcParts,
    /// Only the preview: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub tabs_mode: TcTabsMode,
    /// The keys of the rows to import (`menu<N>`, `left<N>`, `right<N>`, `ftp:<section>`); what
    /// is new or changed when `None` - a tab whose folder is missing only when named.
    #[serde(default)]
    pub only: Option<Vec<String>>,
}

/// The parameters of `import.total_commander_passwords`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TcPasswordsRequest {
    #[serde(default)]
    pub wincmd_ini: Option<String>,
    #[serde(default)]
    pub ftp_ini: Option<String>,
    /// The `ftp:<section>` keys whose passwords to take; every imported one when `None`.
    #[serde(default)]
    pub only: Option<Vec<String>>,
}

/// The parameters of `import.remote_desktop_manager` (RC-050-11).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RdmImportRequest {
    /// The `.rdm` export, written without a master key. Over MCP only a file ending in `.rdm`
    /// that the read guard lets through.
    pub path: String,
    /// Only the preview: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
    /// The keys of the entries to import (RDM's IDs, lower case); every new or changed one
    /// when `None`.
    #[serde(default)]
    pub only: Option<Vec<String>>,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    // Secrets and trust decisions go into and come out of the file: a person's (RC-080-19).
    CommandSpec {
        id: "config.export",
        class: CommandClass::GuiOnly,
        summary: "Export settings, connections, trusted host keys and certificates, hotlist and tabs to a file, unencrypted or sealed with an export password",
    },
    CommandSpec {
        id: "config.import",
        class: CommandClass::GuiOnly,
        summary: "Import an rCommander export: dry_run previews new, changed and conflicting rows; secrets go straight into the vault",
    },
    CommandSpec {
        id: "connections.import_ssh_config",
        class: CommandClass::Write,
        summary: "Import the hosts of an OpenSSH config (~/.ssh/config by default); dry_run only previews; never a secret",
    },
    // A write as the other imports are, its preview included (RC-080-12).
    CommandSpec {
        id: "import.remote_desktop_manager",
        class: CommandClass::Write,
        summary: "Import the SSH connections of a Remote Desktop Manager export (.rdm without a master key); dry_run only previews; never a password",
    },
    // A write as the ssh_config import is, its preview included (RC-080-12).
    CommandSpec {
        id: "import.total_commander",
        class: CommandClass::Write,
        summary: "Import Total Commander's hotlist, tabs, FTP connections, list colours and list font (wincmd.ini, wcx_ftp.ini); dry_run only previews; never a password",
    },
    CommandSpec {
        id: "import.total_commander_passwords",
        class: CommandClass::GuiOnly,
        summary: "Take the FTP passwords of connections imported from Total Commander into the vault",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "config.export" => ExportConfig(ConfigExportRequest),
            "config.import" => ImportConfig(ConfigImportRequest),
            "connections.import_ssh_config" => ImportSshConfig(SshConfigImportRequest),
            "import.remote_desktop_manager" => ImportRemoteDesktopManager(RdmImportRequest),
            "import.total_commander" => ImportTotalCommander(TcImportRequest),
            "import.total_commander_passwords" => ImportTotalCommanderPasswords(TcPasswordsRequest),
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
