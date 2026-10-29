//! A connection's password, private key and passphrase stored and taken in: `secrets.import_key`
//! (`GuiOnly`), and the stores the imports and a quick connection make. The manager's form
//! stores and removes them through `connections.save` (RC-080-25).
//!
//! Storing needs the vault unlocked: with no vault yet the answer is `vault_absent` and the GUI
//! asks for a new master password (`vault.create`) before it stores again; with a locked one it
//! is `vault_locked`. A private key comes only from its file (`secrets.import_key`), opened once
//! with its passphrase to prove both, and is stored with its public half in clear; the file is
//! left as it is. When a removal takes the last stored secret the engine announces
//! [`EngineEvent::VaultUnused`] so the GUI can offer to remove the vault too.

use rc_db::SECRET_COLUMNS;
use rc_vault::{ExposeSecret as _, SecretString};
use tokio::io::AsyncReadExt as _;
use zeroize::Zeroize as _;

use crate::{
    ConnectionId, Engine, EngineError, EngineEvent, MAX_KEY_BYTES, Outcome, SecretKind,
    SecretParam, StoredKey, TreeChange, TreeItem,
    secrets::{field, local_key_path},
};

impl Engine {
    /// Stores a password or a passphrase for the imports and a quick connection; a key comes
    /// from its file.
    pub(crate) async fn set_secret(
        &self,
        id: ConnectionId,
        kind: SecretKind,
        value: &SecretParam,
    ) -> Result<Outcome, EngineError> {
        if kind == SecretKind::Key {
            return Err(EngineError::KeyImportOnly);
        }
        let stored = self.store_secret(id, kind, value.secret()).await?;
        self.forget_remote(id).await;
        Ok(stored)
    }

    /// `secrets.import_key`: the key file - a local one ([`local_key_path`]) - read once, checked
    /// and opened with `passphrase` off the async threads, then the key, its passphrase (only
    /// when the key needs one) and its public half stored in one transaction.
    pub(crate) async fn import_key(
        &self,
        id: ConnectionId,
        path: &str,
        passphrase: Option<&SecretParam>,
    ) -> Result<Outcome, EngineError> {
        // A share is never opened: on Windows that alone sends the person's credentials to it.
        if !local_key_path(path) {
            return Err(EngineError::KeyPathRefused {
                path: path.to_owned(),
            });
        }
        // Absent or locked is answered before the person waits for the key to open.
        self.vault_handle().await?.touch()?;
        let key = read_key_file(path).await?;
        let passphrase = passphrase
            .filter(|passphrase| !passphrase.is_empty())
            .map(|passphrase| passphrase.secret().clone());
        self.store_private_key(id, key, passphrase).await
    }

    /// A private key's text and its passphrase: opened once off the async threads to prove
    /// both, then the key, its passphrase (only when the key needs one) and its public half
    /// stored in one transaction. `secrets.import_key` reads the key from its file, the import
    /// of an export (RC-080-19) from the export.
    pub(crate) async fn store_private_key(
        &self,
        id: ConnectionId,
        key: SecretString,
        passphrase: Option<SecretString>,
    ) -> Result<Outcome, EngineError> {
        let (stored, passphrase) = open_key(&key, passphrase).await?;
        let connection = {
            let _writes = self.inner.secret_writes.lock().await;
            let vault = self.vault_handle().await?;
            let row = id.to_string();
            let key_cipher = vault.encrypt_str(&field(SecretKind::Key, &row), &key)?;
            let passphrase_cipher = passphrase
                .as_ref()
                .map(|passphrase| {
                    vault.encrypt_str(&field(SecretKind::Passphrase, &row), passphrase)
                })
                .transpose()?;
            self.db()?
                .store_connection_key(id, &key_cipher, &stored, passphrase_cipher.as_deref())
                .await
                .map_err(|error| self.secret_write_failed(error.into()))?
        };
        self.remember_secret(&key);
        if let Some(passphrase) = &passphrase {
            self.remember_secret(passphrase);
        }
        tracing::debug!(
            connection = %id,
            fingerprint = %stored.fingerprint,
            "private key taken into the vault"
        );
        self.publish(EngineEvent::ConnectionTree {
            item: TreeItem::Connection(id),
            change: TreeChange::Updated,
        });
        Ok(Outcome::Connection { connection })
    }

    /// Announces [`EngineEvent::VaultUnused`] when a vault exists and no secret is stored any
    /// more: after a secret, a connection or a folder was removed. The removal has happened, so
    /// a failing look is only logged.
    pub(crate) async fn announce_if_vault_unused(&self) {
        let Ok(db) = self.db() else {
            return;
        };
        let unused = async {
            Ok::<_, rc_db::DbError>(
                db.vault_meta().await?.is_some() && db.stored_secrets(SECRET_COLUMNS).await? == 0,
            )
        };
        match unused.await {
            Ok(true) => self.publish(EngineEvent::VaultUnused),
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "cannot tell whether the vault is still in use"),
        }
    }

    /// Encrypts `secret` for its field and stores it; remembered for the MCP mask.
    async fn store_secret(
        &self,
        id: ConnectionId,
        kind: SecretKind,
        secret: &SecretString,
    ) -> Result<Outcome, EngineError> {
        if secret.expose_secret().trim().is_empty() {
            return Err(EngineError::SecretEmpty);
        }
        let connection = {
            let _writes = self.inner.secret_writes.lock().await;
            let vault = self.vault_handle().await?;
            let row = id.to_string();
            let cipher = vault.encrypt_str(&field(kind, &row), secret)?;
            self.db()?
                .set_connection_secret(id, kind, Some(cipher.as_slice()))
                .await
                .map_err(|error| self.secret_write_failed(error.into()))?
        };
        self.remember_secret(secret);
        tracing::debug!(connection = %id, kind = kind.as_str(), "secret stored");
        self.publish(EngineEvent::ConnectionTree {
            item: TreeItem::Connection(id),
            change: TreeChange::Updated,
        });
        Ok(Outcome::Connection { connection })
    }
}

/// A private key's text opened once with `passphrase` off the async threads, to prove both: its
/// public half as stored with it, and the passphrase only when the key needs one.
pub(crate) async fn open_key(
    key: &SecretString,
    passphrase: Option<SecretString>,
) -> Result<(StoredKey, Option<SecretString>), EngineError> {
    let info = {
        let (key, passphrase) = (key.clone(), passphrase.clone());
        tokio::task::spawn_blocking(move || {
            rc_remote::private_key::inspect(
                key.expose_secret(),
                passphrase
                    .as_ref()
                    .map(|passphrase| passphrase.expose_secret()),
            )
        })
        .await
        .map_err(|_| EngineError::KeyFormatUnknown)??
    };
    let passphrase = passphrase.filter(|_| info.encrypted);
    let stored = StoredKey {
        key_type: info.algorithm,
        fingerprint: info.fingerprint,
        comment: info.comment,
        stored: true,
        public_key: info.public_key,
    };
    Ok((stored, passphrase))
}

/// The key file a connection names (`key_path`, a quick connection's key), for its SSH session:
/// [`read_key_file`] once [`local_key_path`] holds; any other path is refused unread, so a
/// stored or imported `\\server\share\key` never makes the engine open a share.
pub(crate) async fn read_local_key_file(path: &str) -> Result<SecretString, EngineError> {
    local_key_file(path, |path| async move { read_key_file(&path).await }).await
}

/// `read` for `path` when it is a local key path; never called for any other.
async fn local_key_file<F, Fut>(path: &str, read: F) -> Result<SecretString, EngineError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<SecretString, EngineError>>,
{
    if !local_key_path(path) {
        return Err(EngineError::KeyPathRefused {
            path: path.to_owned(),
        });
    }
    read(path.to_owned()).await
}

/// The key file at `path`, a regular file of at most [`MAX_KEY_BYTES`] and UTF-8, read once;
/// the read buffer is zeroized. A FIFO or a device is never opened (it would block or never
/// end); the file opened is judged again and read through `take`, so one swapped or grown after
/// the check cannot exceed the limit either.
pub(crate) async fn read_key_file(path: &str) -> Result<SecretString, EngineError> {
    let unreadable = |source| EngineError::KeyFileUnreadable {
        path: path.to_owned(),
        source,
    };
    let not_a_file = || unreadable(std::io::Error::other("not a regular file"));
    if !tokio::fs::metadata(path)
        .await
        .map_err(unreadable)?
        .is_file()
    {
        return Err(not_a_file());
    }
    let file = tokio::fs::File::open(path).await.map_err(unreadable)?;
    if !file.metadata().await.map_err(unreadable)?.is_file() {
        return Err(not_a_file());
    }
    let limit = u64::try_from(MAX_KEY_BYTES).unwrap_or(u64::MAX);
    let mut bytes = Vec::new();
    let read = file
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .await;
    let key = match read {
        Err(source) => Err(unreadable(source)),
        Ok(_) if bytes.len() > MAX_KEY_BYTES => Err(EngineError::KeyFormatUnknown),
        Ok(_) => std::str::from_utf8(&bytes)
            .map(SecretString::from)
            .map_err(|_| EngineError::KeyFormatUnknown),
    };
    bytes.zeroize();
    key
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "secrets_key_tests.rs"]
mod key_tests;

#[cfg(test)]
#[path = "secrets_endpoint_tests.rs"]
mod endpoint_tests;

#[cfg(test)]
#[path = "secrets_import_tests.rs"]
mod import_tests;
