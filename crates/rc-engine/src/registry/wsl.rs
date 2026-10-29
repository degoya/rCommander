//! The installed WSL distributions (RC-010-04).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "wsl.list_distros",
        class: CommandClass::Read,
        summary: "The installed WSL distributions: name, default, version, root share",
    },
    CommandSpec {
        id: "wsl.refresh",
        class: CommandClass::Read,
        summary: "Read the installed WSL distributions again",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// The kept list; read first when there is none.
            "wsl.list_distros" => ListWslDistros,
            /// The list read again and kept; announced as [`crate::EngineEvent::WslDistros`].
            "wsl.refresh" => RefreshWslDistros,
        } }
    };
}
pub(super) use commands;
