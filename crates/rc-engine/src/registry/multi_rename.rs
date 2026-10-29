//! Multi-rename: Total Commander's multi-rename tool - preview, apply as a queued job, undo
//! (RC-020-01); its dialog (Ctrl+M) and the saved rules, the presets (RC-020-02).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "multi_rename.apply",
        class: CommandClass::Write,
        summary: "Rename entries of a folder by masks, case and search and replace, as a queued job logged for undo; refused while the preview has conflicts",
    },
    CommandSpec {
        id: "multi_rename.open_dialog",
        class: CommandClass::Read,
        summary: "Open the multi-rename dialog in the window on a panel's selection, or the entry under its cursor",
    },
    // Destructive as `hotlist.delete` is: what is gone was the person's (RC-080-12).
    CommandSpec {
        id: "multi_rename.preset_delete",
        class: CommandClass::Destructive,
        summary: "Delete a saved multi-rename preset",
    },
    CommandSpec {
        id: "multi_rename.preset_list",
        class: CommandClass::Read,
        summary: "List the saved multi-rename presets by name, each with its rule, and the rule last used",
    },
    CommandSpec {
        id: "multi_rename.preset_rename",
        class: CommandClass::Write,
        summary: "Give a saved multi-rename preset another name",
    },
    CommandSpec {
        id: "multi_rename.preset_save",
        class: CommandClass::Write,
        summary: "Save a multi-rename rule as a preset under a name, replacing one of that name",
    },
    CommandSpec {
        id: "multi_rename.preview",
        class: CommandClass::Read,
        summary: "Preview a multi-rename: each entry's new name, the kind of change and its conflict",
    },
    CommandSpec {
        id: "multi_rename.undo",
        class: CommandClass::Write,
        summary: "Reverse the latest multi-rename as far as it got; refused when an entry no longer fits",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            // The multi-rename commands keep their parameters in a struct of their module.
            "multi_rename.apply" => MultiRenameApply(MultiRenameRequest),
            /// Ctrl+M: the window opens the multi-rename dialog for the panel.
            "multi_rename.open_dialog" => MultiRenameOpenDialog {
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "multi_rename.preset_delete" => MultiRenamePresetDelete { name: String },
            /// No parameters: every preset and the rule last used.
            "multi_rename.preset_list" => MultiRenamePresetList,
            "multi_rename.preset_rename" => MultiRenamePresetRename {
                name: String,
                new_name: String,
            },
            /// The rule's fields default as in `multi_rename.preview`.
            "multi_rename.preset_save" => MultiRenamePresetSave {
                name: String,
                #[serde(default)]
                rule: crate::RenameRule,
            },
            "multi_rename.preview" => MultiRenamePreview(MultiRenameRequest),
            /// No parameters: the latest run of the undo log.
            "multi_rename.undo" => MultiRenameUndo,
        } }
    };
}
pub(super) use commands;
