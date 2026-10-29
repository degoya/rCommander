//! One file: open it as Enter does (RC-010-02), split it into parts and combine them again
//! (RC-020-05).

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::PanelId;

/// The parameters of `file.split`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The file to split, absolute or relative.
    pub path: String,
    /// Bytes per part, the last one shorter.
    pub part_size: u64,
    /// Where the parts go; the file's folder when `None` (the GUI passes the other panel's).
    #[serde(default)]
    pub target_dir: Option<String>,
}

/// The parameters of `file.combine`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombineRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The first part (`.001`) or the `.crc` file, absolute or relative.
    pub path: String,
    /// Where the file goes; the parts' folder when `None`.
    #[serde(default)]
    pub target_dir: Option<String>,
    /// Its name; the `.crc` file's, else the parts' stem, when `None`.
    #[serde(default)]
    pub target_name: Option<String>,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "file.combine",
        class: CommandClass::Write,
        summary: "Put split parts (.001 or .crc) back together into a new file, CRC checked",
    },
    CommandSpec {
        id: "file.open",
        class: CommandClass::Exec,
        summary: "Open the entry under a panel's cursor or a path: show a directory, start a program, open a file in its associated program",
    },
    CommandSpec {
        id: "file.split",
        class: CommandClass::Write,
        summary: "Split a file into parts of a given size with a Total Commander .crc file",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "file.split" => SplitFile(SplitRequest),
            "file.combine" => CombineFiles(CombineRequest),
            /// Enter in Total Commander: a directory is shown in the panel, a program starts, any
            /// other file opens in the program the system associates with it.
            "file.open" => OpenFile {
                #[serde(default)]
                panel: Option<PanelId>,
                /// Absolute, or relative to the panel's directory; the entry under the cursor when
                /// `None`.
                #[serde(default)]
                path: Option<String>,
            },
        } }
    };
}
pub(super) use commands;
