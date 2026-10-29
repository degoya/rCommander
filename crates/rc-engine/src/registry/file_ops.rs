//! The function keys' dialogs and the external viewer and editor (RC-010-07).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "file.open_external",
        class: CommandClass::Exec,
        summary: "Open a file in the viewer or editor of the settings, or the system's program (F3, F4, Shift+F4)",
    },
    CommandSpec {
        id: "ops.copy_to",
        class: CommandClass::Write,
        summary: "Copy to a typed target: into a directory, or one entry under a new name (the F5 dialog)",
    },
    CommandSpec {
        id: "ops.dialog",
        class: CommandClass::Read,
        summary: "Open a file-operation dialog in the window: copy, move, rename, new folder, delete, new text file",
    },
    CommandSpec {
        id: "ops.move_to",
        class: CommandClass::Write,
        summary: "Move to a typed target: into a directory, or one entry renamed (the F6 dialog)",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// F5 to F8, Shift+F4 and the others: the window opens the dialog (RC-010-07).
            "ops.dialog" => FileDialog {
                #[serde(default)]
                panel: Option<PanelId>,
                dialog: FileDialog,
            },
            /// The F5 dialog's target as typed: relative to the panel's own directory; into it when
            /// it is a directory (or ends with a separator or `*.*`, or there are several sources),
            /// else the one source is copied under that path.
            "ops.copy_to" => OpsCopyTo {
                #[serde(default)]
                panel: Option<PanelId>,
                /// As for `ops.copy`.
                #[serde(default)]
                sources: Vec<String>,
                target: String,
                #[serde(default)]
                options: JobOptions,
                /// The queue list the job goes into (F2, RC-010-15); `None` runs it at once, outside
                /// the queue's lists, as Total Commander's OK does (owner, 2026-10-18).
                #[serde(default)]
                queue: Option<QueueListId>,
            },
            /// The F6 dialog's target, read like `ops.copy_to`: a single source to a new path is
            /// renamed.
            "ops.move_to" => OpsMoveTo {
                #[serde(default)]
                panel: Option<PanelId>,
                #[serde(default)]
                sources: Vec<String>,
                target: String,
                #[serde(default)]
                options: JobOptions,
                #[serde(default)]
                queue: Option<QueueListId>,
            },
            /// F3 and F4: the file in the viewer or editor of the settings.
            "file.open_external" => OpenExternal {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, or relative to the panel's directory; the file under the cursor when
                /// `None`.
                #[serde(default)]
                path: Option<String>,
                #[serde(default)]
                program: ExternalProgram,
                /// A missing `path` is made as an empty file first (Shift+F4).
                #[serde(default)]
                create: bool,
            },
        } }
    };
}
pub(super) use commands;
