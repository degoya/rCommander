//! A file's contents as the viewer reads them (RC-020-07).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "files.read_hex",
        class: CommandClass::Read,
        summary: "A capped window of a file as hex rows, as xxd prints them; private keys withheld",
    },
    CommandSpec {
        id: "files.read_text",
        class: CommandClass::Read,
        summary: "A capped window of a file as text in its detected encoding, and whether it was cut",
    },
    CommandSpec {
        id: "panel.toggle_quick_view",
        class: CommandClass::Read,
        summary: "Show the file under a panel's cursor in the other panel (Quick View), or stop",
    },
    CommandSpec {
        id: "view.open",
        class: CommandClass::Read,
        summary: "Open a file in the window's viewer (Lister): text, binary, hex or image",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "files.read_hex" => ReadHex(ReadHexRequest),
            "files.read_text" => ReadText(ReadTextRequest),
            /// F3: the window's viewer (Lister) shows the file; announced as
            /// [`crate::EngineEvent::ViewFile`].
            "view.open" => ViewFile {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, relative to the panel's directory, or a server's location
                /// (`sftp://`, `ftp://`); `None` is the entry under the cursor.
                #[serde(default)]
                path: Option<String>,
            },
            /// Ctrl+Q: the other panel shows the file under this panel's cursor, or its own view
            /// again; announced as [`crate::EngineEvent::QuickView`].
            "panel.toggle_quick_view" => ToggleQuickView {
                #[serde(default)]
                panel: Option<PanelId>,
            },
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "viewer_tests.rs"]
mod tests;
