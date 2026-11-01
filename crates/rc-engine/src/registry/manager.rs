//! The connection manager in the window and the servers and folders of the drive bar (RC-050-04,
//! RC-100-63).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "connections.manager",
        class: CommandClass::Read,
        summary: "Open the connection manager in the window",
    },
    CommandSpec {
        id: "connections.set_favourite",
        class: CommandClass::Write,
        summary: "Show a connection among the servers of the drive bar, or take it away",
    },
    CommandSpec {
        id: "connections.set_folder_favourite",
        class: CommandClass::Write,
        summary: "Show a folder of the connection tree as a button of the drive bar, or take it away",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// Ctrl+F: the window opens the connection manager.
            "connections.manager" => ConnectionManager {},
            /// Whether the connection is one of the drive bar's servers; those of the tree's root
            /// level are by default.
            "connections.set_favourite" => SetFavourite { id: ConnectionId, favourite: bool },
            /// Whether the folder is a button of the drive bar for its servers; those of the
            /// tree's root level are by default (RC-100-63).
            "connections.set_folder_favourite" => SetFolderFavourite { id: FolderId, favourite: bool },
        } }
    };
}
pub(super) use commands;
