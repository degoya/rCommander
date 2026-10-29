//! Terminal sessions (RC-040-01, RC-040-02); a terminal over the whole window (RC-040-05).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "terminal.close",
        class: CommandClass::Destructive,
        summary: "End a terminal session and its program (whatever runs in it is lost)",
    },
    CommandSpec {
        id: "terminal.list",
        class: CommandClass::Read,
        summary: "The open terminal sessions and the shells a new one can run",
    },
    CommandSpec {
        id: "terminal.maximize",
        class: CommandClass::Exec,
        summary: "Lay a panel's terminal over both panels, or back (a shell starts when the panel shows its files)",
    },
    CommandSpec {
        id: "terminal.open",
        class: CommandClass::Exec,
        summary: "Start a local shell in a terminal session, in a panel's folder by default",
    },
    CommandSpec {
        id: "terminal.paste_image",
        class: CommandClass::GuiOnly,
        summary: "Paste the clipboard's image into a terminal as the path of a PNG file its program can read (uploaded to an SSH server)",
    },
    CommandSpec {
        id: "terminal.read_screen",
        class: CommandClass::Read,
        summary: "A terminal's screen (and scrollback) as text, with the cursor position",
    },
    CommandSpec {
        id: "terminal.send_input",
        class: CommandClass::Exec,
        summary: "Type text into a terminal session, as if from the keyboard",
    },
    CommandSpec {
        id: "terminal.show",
        class: CommandClass::Read,
        summary: "Show a terminal session in its panel, in place of the panel's files or other session",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "terminal.close" => CloseTerminal { terminal: TermId },
            "terminal.list" => ListTerminals {},
            /// The panel's terminal over the whole width of the window, or the two panels again;
            /// a panel showing its files turns to its terminal first, as `panel.toggle_terminal`.
            /// Announced as [`crate::EngineEvent::MaximizeTerminal`]; the window keeps the state.
            "terminal.maximize" => MaximizeTerminal {
                /// The active panel by default.
                #[serde(default)]
                panel: Option<PanelId>,
            },
            "terminal.open" => OpenTerminal {
                /// The panel the terminal belongs to and whose folder it starts in; the active one
                /// by default.
                #[serde(default)]
                panel: Option<PanelId>,
                /// A shell id from `terminal.list`; when `None`, a WSL shell if `distro` is given,
                /// else the setting `terminal.default_shell`, or the first shell found.
                #[serde(default)]
                shell: Option<String>,
                /// The WSL distribution of shell `wsl` (`wsl.list_distros`); WSL's default when
                /// `None`. Ignored for every other shell.
                #[serde(default)]
                distro: Option<String>,
                /// The folder: absolute, or relative to the panel's; the panel's folder when
                /// `None`.
                #[serde(default)]
                cwd: Option<String>,
                /// 24 by 80 when `None`; the panel widget resizes it to fit.
                #[serde(default)]
                size: Option<TermSize>,
            },
            /// The clipboard's image as a file the terminal's program can read, its path pasted
            /// (RC-040-08). The engine announces [`crate::EngineEvent::PasteImage`]; the window
            /// reads the clipboard and calls [`crate::Engine::place_terminal_image`].
            "terminal.paste_image" => PasteImage {
                /// The terminal the active panel shows by default.
                #[serde(default)]
                terminal: Option<TermId>,
            },
            "terminal.read_screen" => ReadScreen {
                terminal: TermId,
                /// Lines of history above the screen to include.
                #[serde(default)]
                scrollback: usize,
            },
            "terminal.send_input" => SendInput {
                terminal: TermId,
                /// Sent as typed; `\r` is Enter.
                text: String,
            },
            /// The panel the terminal belongs to shows it.
            "terminal.show" => ShowTerminal { terminal: TermId },
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
