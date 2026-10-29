//! Stored secrets as the engine hands them out, and the rule of when a person is asked.
//!
//! A connection's password, private key and passphrase sit encrypted in `rcommander.db`
//! (`rc_db::SecretKind`, migration 0006). [`Engine::resolve_secret`] is the one way to read one:
//! nothing stored answers `None` without touching the vault, so a person who stores no secret is
//! never asked for a master password; a stored one with the vault locked answers
//! [`EngineError::VaultLocked`] and announces [`crate::EngineEvent::SecretNeeded`] - the engine
//! never asks by itself, the GUI shows the unlock dialog and tries the connection again.
//!
//! A secret enters the engine only as a [`SecretParam`] of a `GuiOnly` command, whose `Debug` and
//! serde form never carry the value. A private key is taken into the vault from its file
//! (`secrets.import_key`): checked and opened once with its passphrase (`rc_remote::private_key`),
//! then stored encrypted with its passphrase, its public half in clear for display, so a backup
//! of `rcommander.db` carries it. An SSH session reads it back with [`Engine::resolve_secret`] and
//! opens it in memory only. Every secret stored or read is remembered as its length and a keyed
//! hash (`secrets/fingerprints.rs`), so [`Engine::scrub_secrets`] can replace it word for word in
//! an MCP answer wherever it turns up - after the vault locked too, while an SSH screen it reached
//! is still open (RC-080-11).
//!
//! Only the person's own reads defer the vault's idle lock: a secret resolved for an MCP call
//! ([`crate::read_guard::from_mcp`]) opens with the key and leaves the lock where it was
//! ([`rc_vault::Use::Agent`]), so an agent cannot keep the vault open.

mod fingerprints;
mod key_path;

use std::{
    fmt,
    sync::{MutexGuard, PoisonError},
};

use rc_vault::{ExposeSecret as _, FieldRef, MasterPassword, SecretString, Use, VaultError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize as _;

pub use rc_db::{SecretFlags, SecretKind, SecretOwner, StoredKey};
pub use rc_remote::{KeyFormat, KeyInfo, MAX_KEY_BYTES};

use crate::{ConnectionId, Engine, EngineError, EngineEvent};

pub(crate) use fingerprints::Fingerprints;
pub(crate) use key_path::local_key_path;

/// The shortest master password `vault.create` and `vault.change_password` accept, in
/// characters. It cannot be recovered, so it has to be worth its weight.
pub const MIN_MASTER_PASSWORD_LEN: usize = 8;

/// What a secret's value is replaced with, in `Debug`, in serde and by [`Engine::scrub_secrets`].
pub const REDACTED: &str = "[redacted]";

/// Known secrets shorter than this are not scrubbed word for word: replacing every `abc` in an
/// answer would garble it and hide nothing.
const MIN_SCRUB_LEN: usize = 4;

/// The lines of a multi-line secret (a private key) are scrubbed one by one from this length on,
/// so a key shown with other line breaks is caught too.
const MIN_SCRUB_LINE_LEN: usize = 16;

/// Which secret of which connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretRef {
    pub connection: ConnectionId,
    pub kind: SecretKind,
}

/// A password, private key or passphrase as a command parameter. Zeroized on drop; `Debug` and
/// serde write `[redacted]` and never the value, so a command that carries one can be logged or
/// serialized safely. Reading one from serde takes the value (a test, a script of the GUI).
#[derive(Clone)]
pub struct SecretParam(SecretString);

impl SecretParam {
    /// Wraps a typed secret; the `String` is zeroized here.
    #[must_use]
    pub fn new(mut value: String) -> Self {
        let kept = SecretString::from(value.as_str());
        value.zeroize();
        Self(kept)
    }

    /// Whether nothing was typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.expose_secret().is_empty()
    }

    /// The value, for the vault.
    pub(crate) fn secret(&self) -> &SecretString {
        &self.0
    }

    /// The value as a master password, checked against the minimum length.
    pub(crate) fn master_password(&self) -> Result<MasterPassword, EngineError> {
        if self.0.expose_secret().chars().count() < MIN_MASTER_PASSWORD_LEN {
            return Err(EngineError::MasterPasswordTooShort {
                min: MIN_MASTER_PASSWORD_LEN,
            });
        }
        Ok(MasterPassword::from(self.0.clone()))
    }
}

impl From<String> for SecretParam {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SecretParam {
    fn from(value: &str) -> Self {
        Self(SecretString::from(value))
    }
}

impl fmt::Debug for SecretParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretParam({REDACTED})")
    }
}

impl PartialEq for SecretParam {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret() == other.0.expose_secret()
    }
}

impl Eq for SecretParam {}

impl Serialize for SecretParam {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(REDACTED)
    }
}

impl<'de> Deserialize<'de> for SecretParam {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

/// Where a secret of `kind` of the connection `row_id` lives: the vault's associated data.
pub(crate) fn field(kind: SecretKind, row_id: &str) -> FieldRef<'_> {
    field_in(rc_db::SECRETS_TABLE, kind, row_id)
}

/// Where a secret of `kind` of row `row_id` of `table` lives: a connection's, or a shared
/// credential's (`rc_db::CREDENTIALS_TABLE`, RC-050-12).
pub(crate) fn field_in<'a>(table: &'static str, kind: SecretKind, row_id: &'a str) -> FieldRef<'a> {
    FieldRef::new(table, kind.column(), row_id)
}

impl From<rc_remote::PrivateKeyError> for EngineError {
    fn from(error: rc_remote::PrivateKeyError) -> Self {
        match error {
            rc_remote::PrivateKeyError::Unknown => Self::KeyFormatUnknown,
            rc_remote::PrivateKeyError::PassphraseNeeded => Self::KeyPassphraseNeeded,
            rc_remote::PrivateKeyError::PassphraseWrong => Self::KeyPassphraseWrong,
        }
    }
}

impl From<VaultError> for EngineError {
    fn from(error: VaultError) -> Self {
        match error {
            VaultError::Locked => Self::VaultLocked,
            other => Self::Vault(other),
        }
    }
}

impl Engine {
    /// The stored secret `secret` refers to, decrypted - from the shared credential the
    /// connection points at (RC-050-12), or asked from the source the person mapped it to
    /// (`secret_sources.rs`); `None` when neither is set - then the vault is not
    /// touched and nobody is asked. Counts as the person's use of the vault, except
    /// in an MCP dispatch.
    ///
    /// # Errors
    ///
    /// [`EngineError::VaultLocked`] when a secret is stored and the vault is locked (announced as
    /// [`EngineEvent::SecretNeeded`], except in an MCP dispatch); the connection is missing; the stored value does not
    /// decrypt (`vault_undecryptable`, `vault_malformed`); no database is attached; or the
    /// database fails; a source that fails ([`EngineError::SecretSource`]).
    pub async fn resolve_secret(
        &self,
        secret: SecretRef,
    ) -> Result<Option<SecretString>, EngineError> {
        self.secret_for(secret, true).await
    }

    /// [`Engine::resolve_secret`], announcing a locked vault ([`EngineEvent::SecretNeeded`]) only
    /// when `announce` is set: a call an agent made over MCP opens no unlock dialog in the window
    /// and answers `vault_locked`. The read defers the idle lock only for the window's own call
    /// (`announce` set and no MCP dispatch).
    pub(crate) async fn secret_for(
        &self,
        secret: SecretRef,
        announce: bool,
    ) -> Result<Option<SecretString>, EngineError> {
        // A shared credential stands in for the connection's own secrets (RC-050-12).
        if let Some(credential) = self.shared_credential(secret.connection).await? {
            let owner = SecretOwner::Credential(credential.id);
            return self.owner_secret(owner, secret, announce).await;
        }
        // A reference to a source comes first; the vault is not touched then (RC-100-34).
        if let Some(reference) = self
            .db()?
            .secret_source(secret.connection, secret.kind)
            .await?
        {
            return self.secret_from_source(&reference).await.map(Some);
        }
        self.vault_secret(secret, announce).await
    }

    /// [`Engine::secret_for`] from the vault alone, whatever a reference says: what an export
    /// carries is what is stored, never a source's value.
    pub(crate) async fn vault_secret(
        &self,
        secret: SecretRef,
        announce: bool,
    ) -> Result<Option<SecretString>, EngineError> {
        let owner = SecretOwner::Connection(secret.connection);
        self.owner_secret(owner, secret, announce).await
    }

    /// `owner`'s stored secret of `secret.kind`, decrypted; a locked vault is announced as
    /// `secret` needed, so the window tries that connection again once it is open.
    async fn owner_secret(
        &self,
        owner: SecretOwner,
        secret: SecretRef,
        announce: bool,
    ) -> Result<Option<SecretString>, EngineError> {
        let db = self.db()?;
        let Some(cipher) = db.stored_secret(owner, secret.kind).await? else {
            return Ok(None);
        };
        let row = owner.row().to_string();
        let vault = self.vault_handle().await?;
        let by = if announce { secret_use() } else { Use::Agent };
        let at = field_in(owner.table(), secret.kind, &row);
        match vault.decrypt_string_for(&at, &cipher, by) {
            Ok(value) => {
                self.remember_secret(&value);
                Ok(Some(value))
            }
            Err(VaultError::Locked) => {
                tracing::debug!(
                    connection = %secret.connection,
                    kind = secret.kind.as_str(),
                    "a stored secret is needed and the vault is locked"
                );
                // Never for an agent's call, whatever the caller asked: no unlock dialog opens
                // for MCP (`read_guard::from_mcp`).
                if announce && !crate::read_guard::from_mcp() {
                    self.publish(EngineEvent::SecretNeeded { secret });
                }
                Err(EngineError::VaultLocked)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// `text` with every secret stored or read in this process replaced by `[redacted]`; `None`
    /// when none occurs. Locked or not: a secret that reached a terminal screen while the vault
    /// was unlocked stays hidden after it locked. The MCP mask calls this on every answer.
    #[must_use]
    pub fn scrub_secrets(&self, text: &str) -> Option<String> {
        self.known_secrets().scrub(text, REDACTED)
    }

    /// Remembers `secret` - its fingerprint, never its value - for [`Engine::scrub_secrets`].
    pub(crate) fn remember_secret(&self, secret: &SecretString) {
        let value = secret.expose_secret();
        if value.chars().count() < MIN_SCRUB_LEN {
            return;
        }
        let mut known = self.known_secrets();
        for fragment in fragments(value) {
            known.remember(fragment);
        }
    }

    fn known_secrets(&self) -> MutexGuard<'_, Fingerprints> {
        self.inner
            .known_secrets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Whose use a secret read in this task is: an agent's inside an MCP dispatch, the person's
/// otherwise.
pub(crate) fn secret_use() -> Use {
    if crate::read_guard::from_mcp() {
        Use::Agent
    } else {
        Use::Person
    }
}

/// The pieces of `secret` that are replaced: the whole value, and for a multi-line one each long
/// line that is not PEM armour.
fn fragments(secret: &str) -> impl Iterator<Item = &str> {
    let lines = secret
        .contains('\n')
        .then(|| {
            secret.lines().map(str::trim).filter(|line| {
                line.chars().count() >= MIN_SCRUB_LINE_LEN && !line.starts_with("-----")
            })
        })
        .into_iter()
        .flatten();
    std::iter::once(secret).chain(lines)
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
