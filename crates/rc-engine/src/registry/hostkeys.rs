//! The SSH host keys a person trusts (RC-050-07).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "hostkeys.answer",
        class: CommandClass::GuiOnly,
        summary: "Answer the question about an unknown SSH host key: connect once, trust, or cancel",
    },
    CommandSpec {
        id: "hostkeys.import_known_hosts",
        class: CommandClass::GuiOnly,
        summary: "Import the host keys of an OpenSSH known_hosts file (the user's own by default)",
    },
    CommandSpec {
        id: "hostkeys.list",
        class: CommandClass::Read,
        summary: "The trusted SSH host keys, optionally filtered by host, key type or fingerprint",
    },
    CommandSpec {
        id: "hostkeys.list_revoked",
        class: CommandClass::Read,
        summary: "The SSH host keys a known_hosts file revoked, optionally filtered",
    },
    // A person's decision as `certificates.remove` is: forgetting a changed key turns the stop
    // into a first sighting (RC-080-12).
    CommandSpec {
        id: "hostkeys.remove",
        class: CommandClass::GuiOnly,
        summary: "Forget a trusted SSH host key; the next connection asks again",
    },
    // Forgetting a revocation lets a key a known_hosts file revoked be trusted again: a person's
    // decision (RC-080-67).
    CommandSpec {
        id: "hostkeys.remove_revoked",
        class: CommandClass::GuiOnly,
        summary: "Remove an SSH host key from the revoked keys; it is judged as any other again",
    },
    CommandSpec {
        id: "hostkeys.replace",
        class: CommandClass::GuiOnly,
        summary: "Replace a stored SSH host key with the changed key a server presented",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "hostkeys.answer" => AnswerHostKey {
                /// The question's id ([`crate::HostKeyQuestion::id`]).
                question: u64,
                answer: HostKeyAnswer,
            },
            "hostkeys.import_known_hosts" => ImportKnownHosts {
                /// The file; `None` reads `~/.ssh/known_hosts` (`%USERPROFILE%\.ssh\known_hosts`).
                #[serde(default)]
                path: Option<String>,
            },
            "hostkeys.list" => ListHostKeys {
                /// Only keys whose host, key type or fingerprint contains this, ignoring case.
                #[serde(default)]
                filter: Option<String>,
            },
            "hostkeys.list_revoked" => ListRevokedHostKeys {
                /// Only keys whose names, key type or fingerprint contain this, ignoring case.
                #[serde(default)]
                filter: Option<String>,
            },
            "hostkeys.remove" => RemoveHostKey {
                /// The stored key's id ([`rc_db::KnownHost::id`]).
                id: i64,
            },
            "hostkeys.remove_revoked" => RemoveRevokedHostKey {
                /// The revoked key's id ([`rc_db::RevokedHostKey::id`]).
                id: i64,
            },
            "hostkeys.replace" => ReplaceHostKey {
                /// The id of the question about a changed key.
                question: u64,
            },
        } }
    };
}
pub(super) use commands;
