//! Compare by content and synchronize dirs (RC-020-04).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "compare.dirs",
        class: CommandClass::Read,
        summary: "Compare two directory trees and propose a synchronization, row by row",
    },
    CommandSpec {
        id: "compare.files",
        class: CommandClass::Read,
        summary: "Compare two files by content: first differing byte, line diff for text",
    },
    CommandSpec {
        id: "compare.open_diff",
        class: CommandClass::Read,
        summary: "Open the diff window for two files, picked as compare.files picks them",
    },
    CommandSpec {
        id: "compare.select_differences",
        class: CommandClass::Read,
        summary: "Select the files that differ in both panels: only on one side, newer, other size",
    },
    CommandSpec {
        id: "sync.apply",
        class: CommandClass::Write,
        summary: "Run a synchronization of two trees as copy and delete jobs of the queue",
    },
    CommandSpec {
        id: "sync.open_dialog",
        class: CommandClass::Read,
        summary: "Open the synchronize-dirs dialog for the two panels' folders",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            // The compare commands keep their parameters in a struct of their module.
            "compare.files" => CompareFiles(FileCompareRequest),
            "compare.dirs" => CompareDirs(DirCompareRequest),
            "compare.open_diff" => OpenDiff {},
            "compare.select_differences" => SelectDifferences {
                /// Files of the same size are also compared byte by byte.
                #[serde(default)]
                by_content: bool,
            },
            "sync.apply" => SyncApply(SyncApplyRequest),
            "sync.open_dialog" => OpenSyncDialog {},
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
