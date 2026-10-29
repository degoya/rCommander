//! The vault and its master password (RC-050-01, RC-050-03).

use super::{CommandClass, CommandSpec};

/// This group's rows, sorted by id.
pub(super) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "vault.change_password",
        class: CommandClass::GuiOnly,
        summary: "Change the master password; every stored secret is re-encrypted in one transaction",
    },
    CommandSpec {
        id: "vault.create",
        class: CommandClass::GuiOnly,
        summary: "Set the master password when the first secret is stored; it cannot be recovered",
    },
    CommandSpec {
        id: "vault.lock",
        class: CommandClass::Read,
        summary: "Lock the vault now: stored secrets need the master password again",
    },
    CommandSpec {
        id: "vault.remove",
        class: CommandClass::GuiOnly,
        summary: "Remove the vault and its master password once no secret is stored",
    },
    CommandSpec {
        id: "vault.status",
        class: CommandClass::Read,
        summary: "Whether a vault exists, whether it is locked, and its idle lock; never key material",
    },
    CommandSpec {
        id: "vault.unlock",
        class: CommandClass::GuiOnly,
        summary: "Unlock the vault with the master password, when a connection needs a stored secret",
    },
];

/// This group's [`super::Command`] variants, each after its id: `command.rs` gathers the
/// groups' lists into the enum, its serde names and [`super::Command::id`].
macro_rules! commands {
    ($next:ident, $rest:tt, { $($done:tt)* }) => {
        $next! { $rest, { $($done)*
            /// No parameters.
            "vault.status" => GetVaultStatus,
            "vault.lock" => LockVault,
            /// The first secret is about to be stored and no vault exists (`vault_absent`).
            "vault.create" => CreateVault { password: SecretParam },
            /// A connection needs a stored secret ([`crate::EngineEvent::SecretNeeded`]).
            "vault.unlock" => UnlockVault { password: SecretParam },
            "vault.change_password" => ChangeMasterPassword {
                current: SecretParam,
                new: SecretParam,
            },
            /// Offered after [`crate::EngineEvent::VaultUnused`]; refused with `vault_in_use` while
            /// a secret is stored.
            "vault.remove" => RemoveVault,
        } }
    };
}
pub(super) use commands;
