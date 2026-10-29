//! The directory hotlist: favourite folders with folders of their own (RC-010-03).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "hotlist.add",
        class: CommandClass::Write,
        summary: "Add a folder to the hotlist, by default the panel's own, optionally with a folder for the other panel",
    },
    CommandSpec {
        id: "hotlist.add_folder",
        class: CommandClass::Write,
        summary: "Add a folder (a submenu) to the hotlist",
    },
    CommandSpec {
        id: "hotlist.delete",
        class: CommandClass::Destructive,
        summary: "Remove a hotlist entry, or a hotlist folder with everything in it",
    },
    CommandSpec {
        id: "hotlist.list",
        class: CommandClass::Read,
        summary: "The hotlist in menu order: id, parent, name, folder, path and target",
    },
    CommandSpec {
        id: "hotlist.move",
        class: CommandClass::Write,
        summary: "Move a hotlist entry or folder to another place or into another hotlist folder",
    },
    CommandSpec {
        id: "hotlist.open",
        class: CommandClass::Read,
        summary: "Show a hotlist entry's folder in a panel, and its target in the other panel",
    },
    CommandSpec {
        id: "hotlist.update",
        class: CommandClass::Write,
        summary: "Change a hotlist entry's name, path or target, or a hotlist folder's name",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "hotlist.list" => ListHotlist {},
            "hotlist.add" => AddHotlist(AddHotlistRequest),
            "hotlist.add_folder" => AddHotlistFolder(AddHotlistFolderRequest),
            "hotlist.update" => UpdateHotlist(UpdateHotlistRequest),
            "hotlist.move" => MoveHotlist(MoveHotlistRequest),
            "hotlist.delete" => DeleteHotlist { id: i64 },
            "hotlist.open" => OpenHotlist(OpenHotlistRequest),
        } }
    };
}
pub(super) use commands;
