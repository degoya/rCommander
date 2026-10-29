//! Stored secrets of the connections (RC-050-03), and the sources a secret can be taken from
//! instead of the vault (RC-100-34).

use serde::{Deserialize, Serialize};

use super::{CommandClass, CommandSpec};
use crate::{ConnectionId, SecretParam};

/// The parameters of `secrets.import_key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportKeyRequest {
    pub connection_id: ConnectionId,
    /// The private key file; its content is stored, the file is left as it is.
    pub path: String,
    /// Needed for an encrypted key (`key_passphrase_needed`, `key_passphrase_wrong`).
    #[serde(default)]
    pub passphrase: Option<SecretParam>,
}

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "secrets.import_key",
        class: CommandClass::GuiOnly,
        summary: "Take a private key file (OpenSSH, PEM, PKCS#8, PuTTY ppk) and its passphrase into the vault",
    },
    CommandSpec {
        id: "secrets.source_map",
        class: CommandClass::GuiOnly,
        summary: "Take a connection's password, key or passphrase from an entry of a secret source instead of the vault",
    },
    CommandSpec {
        id: "secrets.source_search",
        class: CommandClass::Read,
        summary: "Search a secret source: entry ids, titles, users and addresses, never a value",
    },
    CommandSpec {
        id: "secrets.source_unmap",
        class: CommandClass::GuiOnly,
        summary: "Take a connection's secret from the vault again instead of a secret source",
    },
    CommandSpec {
        id: "secrets.sources",
        class: CommandClass::Read,
        summary: "The installed secret sources and whether each answers; with a connection, its references",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            "secrets.import_key" => ImportKey(ImportKeyRequest),
            "secrets.source_map" => MapSecretSource(SecretSourceRef),
            "secrets.source_search" => SearchSecretSource {
                /// `plugin:<id>`, as `secrets.sources` names it.
                source: String,
                query: String,
            },
            "secrets.source_unmap" => UnmapSecretSource {
                connection_id: ConnectionId,
                kind: SecretKind,
            },
            "secrets.sources" => ListSecretSources {
                /// This connection's references too.
                #[serde(default)]
                connection_id: Option<ConnectionId>,
            },
        } }
    };
}
pub(super) use commands;

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
