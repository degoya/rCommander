//! The SSH agents' keys (RC-050-05).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[CommandSpec {
    id: "agent.list_identities",
    class: CommandClass::Read,
    summary: "The keys the running SSH agents hold (ssh-agent, OpenSSH service, Pageant): type, comment, fingerprint; never key bytes",
}];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// No parameters: the agents come from the platform and the setting
            /// `ssh.agent_sources`.
            "agent.list_identities" => ListAgentIdentities,
        } }
    };
}
pub(super) use commands;
