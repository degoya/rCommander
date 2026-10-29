//! The person's answers to MCP calls that wait in the window (RC-010-13), and taking back
//! what one allowed for a session (RC-010-27).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "mcp.answer",
        class: CommandClass::GuiOnly,
        summary: "Answer the question of an MCP call waiting in the window: allow, allow for the session, or deny",
    },
    CommandSpec {
        id: "mcp.revoke",
        class: CommandClass::GuiOnly,
        summary: "Take back every MCP call a person allowed for a session: the next one asks again",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// The person's answer to the question of an MCP call (RC-010-13): the window's dialog
            /// only.
            "mcp.answer" => AnswerMcpQuestion {
                /// The question's id ([`crate::McpQuestion::id`]).
                question: u64,
                answer: crate::McpAnswer,
            },
            /// Takes back what "allow for this session" granted (RC-010-27): the window's
            /// status bar only.
            "mcp.revoke" => RevokeMcpGrants {},
        } }
    };
}
pub(super) use commands;
