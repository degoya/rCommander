//! The SSH sessions of the connections (RC-060-01).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "connections.connect",
        class: CommandClass::Write,
        summary: "Open the SSH session of a connection: host key checked, login by agent, key or password from the vault",
    },
    CommandSpec {
        id: "connections.disconnect",
        class: CommandClass::Write,
        summary: "Close the SSH or FTP session of a connection, or cancel its setup; panels showing the server go back to a local folder",
    },
    CommandSpec {
        id: "connections.session_prefs",
        class: CommandClass::Read,
        summary: "A connection's SSH session settings: auth order, agent key, keepalive, idle close",
    },
    CommandSpec {
        id: "connections.set_session_prefs",
        class: CommandClass::Write,
        summary: "Change a connection's SSH session settings, used by its next session",
    },
    CommandSpec {
        id: "connections.test",
        class: CommandClass::Write,
        summary: "Test a connection: connect, check the host key, log in, disconnect; answers the method and the time",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "connections.connect" => ConnectSession { id: ConnectionId },
            "connections.disconnect" => DisconnectSession { id: ConnectionId },
            "connections.test" => TestConnection { id: ConnectionId },
            "connections.session_prefs" => GetSessionPrefs { id: ConnectionId },
            "connections.set_session_prefs" => SetSessionPrefs {
                id: ConnectionId,
                prefs: SessionPrefs,
            },
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
