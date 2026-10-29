//! The command line under the panels and the panels' quick filter (RC-010-08).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "cmdline.run",
        class: CommandClass::Exec,
        summary: "Run a command line in a panel's folder: cd and a drive move the panel, any other line starts in the system's shell in a console window",
    },
    CommandSpec {
        id: "panel.filter",
        class: CommandClass::Read,
        summary: "Show only the entries of a panel whose names match a filter (* and ? as wildcards), or every entry again",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// Enter in the command line under the panels: `cd`, `chdir` and a drive (`d:`) move
            /// the panel and start nothing; any other line runs in the system's shell (`cmd /c`,
            /// `sh -c`) in the panel's folder, in a console window of its own. Local folders only.
            "cmdline.run" => RunCommandLine {
                #[serde(default)]
                panel: Option<PanelId>,
                /// The line as typed: at most [`crate::command_line::MAX_LINE`] characters, no
                /// control characters.
                line: String,
                /// Shift+Enter: the console window stays open after the line ends (`cmd /k`).
                #[serde(default)]
                keep_open: bool,
            },
            /// Ctrl+S in Total Commander (quick filter): only the entries whose names match
            /// `pattern` show, until it is lifted or the panel shows another directory.
            "panel.filter" => Filter {
                #[serde(default)]
                panel: Option<PanelId>,
                /// `*` and `?` match the whole name, a text without them any part of it, ignoring
                /// case. `None` or blank shows every entry; from a key without one (Ctrl+S) the
                /// window asks.
                #[serde(default)]
                pattern: Option<String>,
                /// Ctrl+Shift+S: the panel's last filter again; `pattern` is then ignored.
                #[serde(default)]
                last: bool,
            },
        } }
    };
}
pub(super) use commands;
