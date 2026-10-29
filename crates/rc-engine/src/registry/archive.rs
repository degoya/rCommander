//! Archives in a panel: list, test (RC-030-02). One opens as a folder does: `panel.open_entry`
//! (Ctrl+PageDown, whatever its name), `file.open` (Enter) and `panel.navigate`.

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "archive.list",
        class: CommandClass::Read,
        summary: "The entries of a directory inside an archive, with packed size, encrypted and unsafe-name marks",
    },
    CommandSpec {
        id: "archive.test",
        class: CommandClass::Read,
        summary: "Test archives in the background: read every entry and check its CRC; damaged and encrypted entries are listed",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "archive.list" => ArchiveList {
                #[serde(default)]
                panel: Option<PanelId>,
                /// The archive file, absolute or relative to the panel's directory; `None` is the
                /// archive the panel shows.
                #[serde(default)]
                path: Option<String>,
                /// The directory inside the archive (`/` is its root); `None` is the root, or the
                /// panel's directory when the panel is inside that archive.
                #[serde(default)]
                dir: Option<String>,
            },
            /// Alt+Shift+F9: one background job per archive, every entry read to its end.
            "archive.test" => ArchiveTest {
                #[serde(default)]
                panel: Option<PanelId>,
                /// The archive files; empty is the archive the panel shows, else the selected
                /// archives (or the one under the cursor).
                #[serde(default)]
                paths: Vec<String>,
            },
        } }
    };
}
pub(super) use commands;
