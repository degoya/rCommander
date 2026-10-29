//! Shells on SSH servers in a panel's terminal (RC-060-03).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[CommandSpec {
    id: "terminal.open_ssh",
    class: CommandClass::Exec,
    summary: "Start a shell on an SSH connection's server in a terminal session, after its login script",
}];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "terminal.open_ssh" => OpenSshTerminal {
                /// The SSH connection whose server the shell runs on.
                connection_id: ConnectionId,
                /// The panel the terminal belongs to; the active one by default.
                #[serde(default)]
                panel: Option<PanelId>,
                /// The server folder to `cd` into after the login script; the connection's start
                /// directory, or the server's default, when `None`.
                #[serde(default)]
                cwd: Option<String>,
                /// 24 by 80 when `None`; the panel widget resizes it to fit.
                #[serde(default)]
                size: Option<TermSize>,
            },
        } }
    };
}
pub(super) use commands;
