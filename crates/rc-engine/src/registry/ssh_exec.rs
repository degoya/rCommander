//! One command on an SSH connection, its output and its end (RC-060-04).

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::ConnectionId;

/// The parameters of `ssh.exec`. Its `Debug` shows the command masked and only the length of
/// the input: a request may carry a secret and is logged.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshExecRequest {
    pub connection_id: ConnectionId,
    /// Run by the user's login shell on the server, exactly as given.
    pub command: String,
    /// Text sent to the command's standard input, which is then closed; closed at once when
    /// `None`.
    #[serde(default)]
    pub stdin: Option<String>,
    /// 60 by default, 1 to 3600.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Bytes kept of each stream: 1 MiB by default, at most 4 MiB.
    #[serde(default)]
    pub max_output_bytes: Option<usize>,
}

impl fmt::Debug for SshExecRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshExecRequest")
            .field("connection_id", &self.connection_id)
            .field("command", &rc_db::redact::redact_text(&self.command))
            .field("stdin_bytes", &self.stdin.as_ref().map(String::len))
            .field("timeout_secs", &self.timeout_secs)
            .field("max_output_bytes", &self.max_output_bytes)
            .finish()
    }
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[CommandSpec {
    id: "ssh.exec",
    class: CommandClass::Exec,
    summary: "Run one command on an SSH connection without a terminal: stdout, stderr and the exit code, under a time and size limit",
}];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "ssh.exec" => SshExec(SshExecRequest),
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "ssh_exec_tests.rs"]
mod tests;
