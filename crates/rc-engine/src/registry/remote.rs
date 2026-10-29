//! An entry on any backend by its location, `ftp://<connection>/<path>` (RC-070-01) and
//! `sftp://<connection>/<path>` (RC-060-02) included; the quick connection to an SSH server
//! that is not in the tree (Ctrl+N, RC-060-02), `GuiOnly`.
//! Testing an FTP connection is `connections.test` (RC-060-01), which calls
//! `Engine::test_ftp_connection` for a kind that is not SSH.

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::{PanelId, SecretParam};

/// The parameters of `connections.quick_connect`; every field may be left out, so the keymap
/// binds the command bare and the window asks for them (its dialog).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickConnectRequest {
    /// The panel that shows the server; the active one when `None`.
    #[serde(default)]
    pub panel: Option<PanelId>,
    #[serde(default)]
    pub host: String,
    /// 22 when `None`.
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub password: Option<SecretParam>,
    /// A private key file, read when the login needs it.
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub passphrase: Option<SecretParam>,
    /// Save as a connection of this name, its secrets into the vault.
    #[serde(default)]
    pub save_as: Option<String>,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "connections.quick_connect",
        class: CommandClass::GuiOnly,
        summary: "Connect to an SSH server by host, user and password or key, saved or not, and show it",
    },
    CommandSpec {
        id: "files.stat",
        class: CommandClass::Read,
        summary: "One entry by its path, local, ftp:// or sftp://<connection>/<path>: kind, size, time",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// Ctrl+N: an SSH server that is not in the tree (RC-060-02); the window's dialog only.
            "connections.quick_connect" => QuickConnect(QuickConnectRequest),
            "files.stat" => Stat {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, relative to the panel's directory, or `ftp://` / `sftp://` plus
                /// `<connection>/<path>`.
                path: String,
            },
        } }
    };
}
pub(super) use commands;
