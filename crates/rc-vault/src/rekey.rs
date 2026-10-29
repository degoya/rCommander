//! A new master password: every secret re-encrypted under the new key, in one transaction.

use rc_db::RekeyError;

use crate::{FieldRef, MasterPassword, Result, Vault, VaultError, aead, vault::new_key};

impl Vault {
    /// Replaces the master password: checks `current`, derives a key for `new` under a fresh
    /// salt (with [`crate::VaultOptions::kdf`], so a change also raises old parameters), and
    /// re-encrypts every value of the secret columns and the `vault_meta` row in one SQLite
    /// transaction. Leaves the vault unlocked under the new key. Answers how many values were
    /// re-encrypted. The old values are overwritten in the file, not only unlinked: the database
    /// runs with `secure_delete` and checkpoints its WAL after the change (`rc-db`).
    ///
    /// # Errors
    ///
    /// [`VaultError::Absent`], [`VaultError::EmptyPassword`], [`VaultError::WrongPassword`],
    /// [`VaultError::Undecryptable`] or [`VaultError::Malformed`] for a stored value the current
    /// key does not open, or the database fails. Then nothing changed: every value still opens
    /// with the current password, and the key in memory is the one before.
    pub async fn change_password(
        &self,
        current: &MasterPassword,
        new: &MasterPassword,
    ) -> Result<usize> {
        if new.is_empty() {
            return Err(VaultError::EmptyPassword);
        }
        let _ops = self.shared.ops.lock().await;
        let (old_key, _) = self.verified_key(current).await?;
        let (meta, next_key) = new_key(new, self.shared.options.kdf).await?;
        #[cfg(test)]
        let fail_after = *self
            .shared
            .fail_rekey_after
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(test)]
        let mut done = 0_usize;
        let rewritten = self
            .shared
            .db
            .rekey_secrets(
                self.shared.options.secret_columns,
                &meta,
                |column, row_id, value| {
                    #[cfg(test)]
                    {
                        if fail_after == Some(done) {
                            return Err(VaultError::Interrupted);
                        }
                        done += 1;
                    }
                    let aad = FieldRef::new(column.table, column.column, row_id).associated_data();
                    let plaintext = aead::open(&old_key, &aad, value)?;
                    aead::seal(&next_key, &aad, &plaintext)
                },
            )
            .await
            .map_err(|error| match error {
                RekeyError::Db(error) => VaultError::Db(error),
                RekeyError::Rewrite(error) => error,
            })?;
        self.shared.db.hold_vault_key(Some(meta.salt));
        let now = self.now();
        self.machine().unlocked(next_key, now);
        Ok(rewritten)
    }
}

#[cfg(test)]
#[path = "rekey_tests.rs"]
mod tests;
