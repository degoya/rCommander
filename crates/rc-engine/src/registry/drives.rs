//! The drive bar's entries: drives, portable devices (RC-100-61), WSL distributions, servers
//! (RC-010-05); network drives mapped and disconnected (RC-010-16); the open connections
//! (RC-110-38).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "drives.connections",
        class: CommandClass::Read,
        summary: "The connections with a session open now (SSH, FTP, FTPS), each with the folder last shown there",
    },
    CommandSpec {
        id: "drives.list",
        class: CommandClass::Read,
        summary: "The drive bar: drives with kind, volume and free space, portable devices, WSL distributions, servers",
    },
    CommandSpec {
        id: "drives.map_network",
        class: CommandClass::GuiOnly,
        summary: "Map a network share to a free drive letter (on macOS: mount it under /Volumes); the system asks the person for the credentials",
    },
    CommandSpec {
        id: "drives.unmap_network",
        class: CommandClass::Destructive,
        summary: "Disconnect a network drive; with open files only when forced, which may lose unsaved data",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// The window's quick connections are left out for MCP (RC-110-38).
            "drives.connections" => ListOpenConnections,
            /// Network drives are asked for their space only with `refresh`, which also reads the
            /// WSL distributions again and is announced as [`crate::EngineEvent::Drives`].
            "drives.list" => ListDrives {
                #[serde(default)]
                refresh: bool,
            },
            /// `remote` is `\\server\share[\...]` or `smb://server/share`, `letter` a free drive
            /// letter (`Z`, `Z:`); `reconnect` keeps the drive across sign-ins. The system asks
            /// for the credentials. On macOS the share is mounted under `/Volumes`, and `letter`
            /// and `reconnect` are not used (RC-110-40).
            "drives.map_network" => MapNetworkDrive {
                remote: String,
                #[serde(default)]
                letter: String,
                #[serde(default)]
                reconnect: bool,
            },
            /// `drive` is a network drive's letter (`Z`, `Z:`), on macOS its mount point;
            /// `force` disconnects it even with files open on it.
            "drives.unmap_network" => UnmapNetworkDrive {
                drive: String,
                #[serde(default)]
                force: bool,
            },
        } }
    };
}
pub(super) use commands;
