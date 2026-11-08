//! The column sets (RC-100-90): the file list's own columns, saved under a name, and the folder
//! masks that switch a panel to them. Reading, saving, ordering and deleting them reach MCP; a
//! panel shows one with `panel.set_view`.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "column_sets.create",
        class: CommandClass::Write,
        summary: "Save a new column set: a name, its columns (field, title, width, alignment) and folder masks",
    },
    CommandSpec {
        id: "column_sets.delete",
        class: CommandClass::Destructive,
        summary: "Delete a column set; a panel or tab showing it shows the full view",
    },
    CommandSpec {
        id: "column_sets.get",
        class: CommandClass::Read,
        summary: "One column set: its name, columns and folder masks",
    },
    CommandSpec {
        id: "column_sets.list",
        class: CommandClass::Read,
        summary: "The column sets in their order, which also decides whose folder mask wins",
    },
    CommandSpec {
        id: "column_sets.move",
        class: CommandClass::Write,
        summary: "Give a column set another place in the list",
    },
    CommandSpec {
        id: "column_sets.rename",
        class: CommandClass::Write,
        summary: "Give a column set another name",
    },
    CommandSpec {
        id: "column_sets.update",
        class: CommandClass::Write,
        summary: "Change a column set's columns and folder masks",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "column_sets.list" => ListColumnSets {},
            "column_sets.get" => GetColumnSet { id: ColumnSetId },
            /// 1 to 16 columns, 0 to 32 folder masks; at most 64 sets.
            "column_sets.create" => CreateColumnSet { name: String, spec: ColumnSetSpec },
            "column_sets.update" => UpdateColumnSet { id: ColumnSetId, spec: ColumnSetSpec },
            "column_sets.rename" => RenameColumnSet { id: ColumnSetId, name: String },
            /// `position` counts from 0 among all sets; beyond the end is the last place.
            "column_sets.move" => MoveColumnSet { id: ColumnSetId, position: usize },
            "column_sets.delete" => DeleteColumnSet { id: ColumnSetId },
        } }
    };
}
pub(super) use commands;
