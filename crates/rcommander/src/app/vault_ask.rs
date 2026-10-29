//! The vault's question before a secret is stored or read, shared by the connection manager's
//! prompt, the database dialog's export and import and Quick Connect: "set a master password"
//! (typed twice) when no vault exists, "unlock" when one is locked, and the command that answers
//! it.

use rc_engine::{Command, SecretParam, VaultStateKind};
use rc_vault::{ExposeSecret as _, SecretString};

/// What the vault asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// No vault yet: a new master password, twice.
    Create,
    Unlock,
}

impl Ask {
    /// The question a vault in `state` needs before a secret is stored; none for an open one.
    pub fn for_state(state: VaultStateKind) -> Option<Self> {
        match state {
            VaultStateKind::Absent => Some(Self::Create),
            VaultStateKind::Locked => Some(Self::Unlock),
            VaultStateKind::Unlocked => None,
        }
    }

    /// The question an engine refusal asks: the vault was locked or removed meanwhile; none for
    /// any other refusal.
    pub fn for_code(code: &str) -> Option<Self> {
        match code {
            "vault_absent" => Some(Self::Create),
            "vault_locked" => Some(Self::Unlock),
            _ => None,
        }
    }

    /// The state of the vault that asks this.
    pub fn state(self) -> VaultStateKind {
        match self {
            Self::Create => VaultStateKind::Absent,
            Self::Unlock => VaultStateKind::Locked,
        }
    }
}

/// Why the typed master password cannot be sent; each dialog words it its own way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsent {
    /// Nothing typed.
    Empty,
    /// A new master password and its repetition differ.
    Mismatch,
}

/// The command that answers `ask` with `master`, a new one only when `repeat` is the same.
pub fn vault_command(
    ask: Ask,
    master: &SecretString,
    repeat: &SecretString,
) -> Result<Command, Unsent> {
    let typed = master.expose_secret();
    if typed.is_empty() {
        return Err(Unsent::Empty);
    }
    let password = SecretParam::from(typed);
    match ask {
        Ask::Create if typed != repeat.expose_secret() => Err(Unsent::Mismatch),
        Ask::Create => Ok(Command::CreateVault { password }),
        Ask::Unlock => Ok(Command::UnlockVault { password }),
    }
}

#[cfg(test)]
#[path = "vault_ask_tests.rs"]
mod tests;
