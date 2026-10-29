//! The drive bar's entries: drives, WSL distributions, servers (RC-010-05); network drives
//! mapped and disconnected (RC-010-16).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "drives.list",
        class: CommandClass::Read,
        summary: "The drive bar: drives with kind, volume and free space, WSL distributions, servers",
    },
    CommandSpec {
        id: "drives.map_network",
        class: CommandClass::GuiOnly,
        summary: "Map a network share to a free drive letter; Windows asks the person for the credentials",
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
            /// Network drives are asked for their space only with `refresh`, which also reads the
            /// WSL distributions again and is announced as [`crate::EngineEvent::Drives`].
            "drives.list" => ListDrives {
                #[serde(default)]
                refresh: bool,
            },
            /// `remote` is `\\server\share[\...]`, `letter` a free drive letter (`Z`, `Z:`);
            /// `reconnect` keeps the drive across sign-ins. Windows asks for the credentials.
            "drives.map_network" => MapNetworkDrive {
                remote: String,
                letter: String,
                #[serde(default)]
                reconnect: bool,
            },
            /// `drive` is a network drive's letter (`Z`, `Z:`); `force` disconnects it even
            /// with files open on it.
            "drives.unmap_network" => UnmapNetworkDrive {
                drive: String,
                #[serde(default)]
                force: bool,
            },
        } }
    };
}
pub(super) use commands;
