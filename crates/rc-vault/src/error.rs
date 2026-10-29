//! The vault's errors, each with a stable code. No message carries a password, a key or a
//! plaintext.

use rc_db::DbError;

/// What went wrong in the vault.
///
/// [`VaultError::code`] is the stable form: the GUI shows the message under `error.<code>`, the
/// MCP server answers it to agents.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VaultError {
    /// No vault was created yet, so there is nothing to unlock.
    #[error("no vault exists yet")]
    Absent,
    /// `create` on a database that has a vault.
    #[error("a vault exists already")]
    Exists,
    /// The key is not in memory; only the GUI can unlock the vault.
    #[error("the vault is locked")]
    Locked,
    /// The master password does not open the vault. Says nothing about which part was wrong.
    #[error("wrong master password")]
    WrongPassword,
    /// An empty master password is not accepted for a new vault.
    #[error("the master password is empty")]
    EmptyPassword,
    /// A stored value is not in the vault's format (too short, unknown version).
    #[error("a stored secret is not in the vault's format")]
    Malformed,
    /// A stored value does not decrypt: another key, another field, or damaged bytes.
    #[error("a stored secret does not decrypt here")]
    Undecryptable,
    /// The stored key-derivation parameters are not usable.
    #[error("the vault's key derivation parameters are not usable")]
    BadParameters,
    /// The key derivation stopped before it finished.
    #[error("the key derivation was interrupted")]
    Interrupted,
    /// `remove` while secrets are still stored.
    #[error("{secrets} secrets are still stored in the vault")]
    InUse { secrets: u64 },
    /// Another process changed the master password since this one unlocked: a secret written
    /// with the old key would never open again, so the write was refused and the vault locked.
    #[error("the master password was changed elsewhere; unlock the vault again")]
    Changed,
    /// The database failed.
    #[error(transparent)]
    Db(#[from] DbError),
}

impl VaultError {
    /// Every code, in declaration order.
    pub const ALL_CODES: &[&str] = &[
        "vault_absent",
        "vault_exists",
        "vault_locked",
        "vault_wrong_password",
        "vault_empty_password",
        "vault_malformed",
        "vault_undecryptable",
        "vault_bad_parameters",
        "vault_interrupted",
        "vault_in_use",
        "vault_changed",
        "vault_database",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Absent => "vault_absent",
            Self::Exists => "vault_exists",
            Self::Locked => "vault_locked",
            Self::WrongPassword => "vault_wrong_password",
            Self::EmptyPassword => "vault_empty_password",
            Self::Malformed => "vault_malformed",
            Self::Undecryptable => "vault_undecryptable",
            Self::BadParameters => "vault_bad_parameters",
            Self::Interrupted => "vault_interrupted",
            Self::InUse { .. } => "vault_in_use",
            Self::Changed => "vault_changed",
            Self::Db(_) => "vault_database",
        }
    }
}

/// The result of every vault call.
pub type Result<T, E = VaultError> = std::result::Result<T, E>;

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
