//! rCommander's vault: the key derivation, the field encryption and the lock (ADR 0002).
//!
//! Secrets live in ordinary columns of `rcommander.db`, each value encrypted on its own. The key
//! is derived from the master password with argon2id; salt, parameters and a verifier sit in
//! the `vault_meta` row. No row, no vault - and then nobody is ever asked for a master password.
//! The key exists only in memory and only while the vault is unlocked; [`Vault::lock`], the idle
//! lock and dropping the [`Vault`] zeroize it.
//!
//! - `kdf`: argon2id and the in-memory key.
//! - `aead`: one value as `v1 | nonce | ciphertext`, XChaCha20-Poly1305 with a fresh 192-bit
//!   random nonce per write and `"<table>/<column>/<row id>"` as associated data, so a value
//!   copied into another row or column does not decrypt there.
//! - `state`: `Absent`, `Locked`, `Unlocked`, the idle lock (deferred by the person's use only),
//!   the longest time unlocked and the delay after failed attempts.
//! - `rekey`: a new master password re-encrypts every secret column in one transaction.
//! - `envelope`: an export sealed with a password of its own (RC-080-19), never the vault's key.
//!
//! The crate knows nothing of connections: which columns hold secrets is
//! [`rc_db::SECRET_COLUMNS`], and when to ask for the password is the GUI's decision. Unlocking
//! is never offered over MCP.

#![warn(unreachable_pub)]

mod aead;
mod envelope;
mod error;
mod kdf;
mod rekey;
mod state;
mod types;
mod vault;

pub use envelope::{EXPORT_AAD, SealedExport, open_export, seal_export};
pub use error::{Result, VaultError};
pub use kdf::KdfParams;
pub use state::{
    Clock, DEFAULT_IDLE_LOCK_MINUTES, DEFAULT_MAX_UNLOCKED_HOURS, IDLE_LOCK_KEY, MAX_UNLOCKED_KEY,
    ManualClock, SystemClock, Use, VaultState, VaultStateKind, VaultStatus,
};
pub use types::{ExposeSecret, FieldRef, MasterPassword, SecretBytes, SecretString};
pub use vault::{Vault, VaultOptions};
