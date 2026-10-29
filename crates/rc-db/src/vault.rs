//! The vault's rows: `vault_meta` and the columns that hold encrypted secrets.
//!
//! The database knows where secrets are and moves bytes; `rc-vault` knows the key and the
//! cipher. Re-encrypting every secret under a new master password is one call here
//! ([`Db::rekey_secrets`]): one transaction, the rewrite of each value handed to the caller, so
//! a failure anywhere leaves every value and the old `vault_meta` row as they were; afterwards
//! the WAL is checkpointed and truncated, so with `secure_delete` the old values are gone from the
//! file and its WAL.
//!
//! Two processes may share the file (the window and a headless `rcommander mcp`, or two
//! windows). One that changed the master password leaves the other holding the old key; a secret
//! that one wrote with it would never open again. So the vault tells the database the salt of the
//! key it holds ([`Db::hold_vault_key`]), and every write of a secret column checks it against
//! `vault_meta` inside its own transaction ([`DbError::VaultChanged`]) - RC-080-11.

use std::sync::{Arc, PoisonError};

use sqlx::{AssertSqlSafe, Connection as _, SqliteConnection};

use crate::{
    CREDENTIALS_TABLE, Db, DbError, Result, SECRETS_ROW_ID, SECRETS_TABLE,
    login_scripts::{SCRIPT_SECRET_COLUMN, SCRIPT_SECRETS_TABLE},
};

/// A column whose values are vault ciphertexts (`v1 | nonce | ciphertext`).
///
/// `row_id` names the column whose value (as text) is the row id in the ciphertext's associated
/// data, `"<table>/<column>/<row id>"`. The table has to be a rowid table. All three are SQL
/// identifiers: ASCII letters, digits and `_`, not starting with a digit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretColumn {
    pub table: &'static str,
    pub column: &'static str,
    pub row_id: &'static str,
}

/// Every column that holds secrets, re-encrypted when the master password changes: a
/// connection's password, private key and passphrase (migration 0006), a login script's own
/// secrets (migration 0009) and a shared credential's (migration 0018).
pub const SECRET_COLUMNS: &[SecretColumn] = &[
    SecretColumn {
        table: SECRETS_TABLE,
        column: "password_enc",
        row_id: SECRETS_ROW_ID,
    },
    SecretColumn {
        table: SECRETS_TABLE,
        column: "key_enc",
        row_id: SECRETS_ROW_ID,
    },
    SecretColumn {
        table: SECRETS_TABLE,
        column: "passphrase_enc",
        row_id: SECRETS_ROW_ID,
    },
    SecretColumn {
        table: SCRIPT_SECRETS_TABLE,
        column: SCRIPT_SECRET_COLUMN,
        row_id: "id",
    },
    SecretColumn {
        table: CREDENTIALS_TABLE,
        column: "password_enc",
        row_id: SECRETS_ROW_ID,
    },
    SecretColumn {
        table: CREDENTIALS_TABLE,
        column: "key_enc",
        row_id: SECRETS_ROW_ID,
    },
    SecretColumn {
        table: CREDENTIALS_TABLE,
        column: "passphrase_enc",
        row_id: SECRETS_ROW_ID,
    },
];

/// What [`Db::remove_vault_meta`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultRemoval {
    /// The row is gone: no vault, so no master password is asked any more.
    Removed,
    /// There was no row.
    Absent,
    /// Secrets are still stored; the row stays.
    InUse { secrets: u64 },
}

/// The `vault_meta` row: the key derivation's parameters and salt and the password check.
/// Nothing in it is secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultMeta {
    /// The vault format.
    pub version: i64,
    /// The key derivation, `argon2id`.
    pub kdf: String,
    pub m_cost_kib: i64,
    pub t_cost: i64,
    pub p_cost: i64,
    pub salt: Vec<u8>,
    /// A fixed plaintext encrypted under the derived key.
    pub verifier: Vec<u8>,
}

/// Why [`Db::rekey_secrets`] stopped; nothing was changed either way.
#[derive(Debug)]
pub enum RekeyError<E> {
    /// The database failed.
    Db(DbError),
    /// The caller's rewrite of one value failed.
    Rewrite(E),
}

impl<E> From<sqlx::Error> for RekeyError<E> {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(DbError::Sqlx(error))
    }
}

type MetaRow = (i64, String, i64, i64, i64, Vec<u8>, Vec<u8>);

impl Db {
    /// The vault's row, or `None` when no vault was ever created.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn vault_meta(&self) -> Result<Option<VaultMeta>> {
        let row: Option<MetaRow> = sqlx::query_as(
            "SELECT version, kdf, m_cost_kib, t_cost, p_cost, salt, verifier \
             FROM vault_meta WHERE id = 1",
        )
        .fetch_optional(self.readers())
        .await?;
        Ok(row.map(
            |(version, kdf, m_cost_kib, t_cost, p_cost, salt, verifier)| VaultMeta {
                version,
                kdf,
                m_cost_kib,
                t_cost,
                p_cost,
                salt,
                verifier,
            },
        ))
    }

    /// Stores the row of a new vault; `false` when a vault exists already (it is kept).
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn insert_vault_meta(&self, meta: &VaultMeta) -> Result<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query(
            "INSERT INTO vault_meta \
             (id, version, kdf, m_cost_kib, t_cost, p_cost, salt, verifier) \
             VALUES (1, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (id) DO NOTHING",
        )
        .bind(meta.version)
        .bind(meta.kdf.clone())
        .bind(meta.m_cost_kib)
        .bind(meta.t_cost)
        .bind(meta.p_cost)
        .bind(meta.salt.clone())
        .bind(meta.verifier.clone())
        .execute(&mut *writer)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Rewrites every non-null value of `columns` with `rewrite(column, row id, value)` and
    /// replaces the `vault_meta` row with `meta`, all in one `BEGIN IMMEDIATE` transaction on
    /// the writer. Answers how many values were rewritten.
    ///
    /// # Errors
    ///
    /// The first failing `rewrite` ([`RekeyError::Rewrite`]), a bad identifier in `columns`, no
    /// vault row, or a failing query ([`RekeyError::Db`]). The transaction is rolled back: every
    /// value and the old row stay as they were.
    pub async fn rekey_secrets<E, F>(
        &self,
        columns: &[SecretColumn],
        meta: &VaultMeta,
        mut rewrite: F,
    ) -> Result<usize, RekeyError<E>>
    where
        F: FnMut(&SecretColumn, &str, &[u8]) -> Result<Vec<u8>, E>,
    {
        for column in columns {
            for name in [column.table, column.column, column.row_id] {
                checked_identifier(name).map_err(RekeyError::Db)?;
            }
        }
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let mut rewritten = 0;
        for column in columns {
            let SecretColumn {
                table,
                column: name,
                row_id,
            } = *column;
            let rows: Vec<(i64, String, Vec<u8>)> = sqlx::query_as(AssertSqlSafe(format!(
                "SELECT rowid, CAST(\"{row_id}\" AS TEXT), \"{name}\" FROM \"{table}\" \
                 WHERE \"{name}\" IS NOT NULL"
            )))
            .fetch_all(&mut *tx)
            .await?;
            // Shared, not copied, by every row's statement.
            let update: Arc<str> =
                format!("UPDATE \"{table}\" SET \"{name}\" = ? WHERE rowid = ?").into();
            for (rowid, id, value) in rows {
                let next = rewrite(column, &id, &value).map_err(RekeyError::Rewrite)?;
                sqlx::query(AssertSqlSafe(Arc::clone(&update)))
                    .bind(next)
                    .bind(rowid)
                    .execute(&mut *tx)
                    .await?;
                rewritten += 1;
            }
        }
        let done = sqlx::query(
            "UPDATE vault_meta SET version = ?, kdf = ?, m_cost_kib = ?, t_cost = ?, \
             p_cost = ?, salt = ?, verifier = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = 1",
        )
        .bind(meta.version)
        .bind(meta.kdf.clone())
        .bind(meta.m_cost_kib)
        .bind(meta.t_cost)
        .bind(meta.p_cost)
        .bind(meta.salt.clone())
        .bind(meta.verifier.clone())
        .execute(&mut *tx)
        .await?;
        if done.rows_affected() != 1 {
            return Err(RekeyError::Db(DbError::NoVault));
        }
        tx.commit().await?;
        checkpoint(&mut writer).await;
        Ok(rewritten)
    }

    /// Secret writes from now on require `vault_meta.salt` to be `salt`, the salt of the key the
    /// vault holds; `None` (no vault, or no key yet) checks nothing.
    pub fn hold_vault_key(&self, salt: Option<Vec<u8>>) {
        *self
            .vault_key
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = salt;
    }

    /// [`DbError::VaultChanged`] when this process holds a vault key and `vault_meta` no longer
    /// has its salt. Called inside the transaction of every secret write, after `BEGIN
    /// IMMEDIATE`, so no password change of another process can come between the check and the
    /// write.
    pub(crate) async fn check_vault_key(&self, connection: &mut SqliteConnection) -> Result<()> {
        let held = self
            .vault_key
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(held) = held else {
            return Ok(());
        };
        let current: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT salt FROM vault_meta WHERE id = 1")
                .fetch_optional(&mut *connection)
                .await?;
        if current.as_deref() == Some(held.as_slice()) {
            Ok(())
        } else {
            Err(DbError::VaultChanged)
        }
    }

    /// How many values `columns` hold: the stored secrets.
    ///
    /// # Errors
    ///
    /// A bad identifier in `columns`, or the read fails.
    pub async fn stored_secrets(&self, columns: &[SecretColumn]) -> Result<u64> {
        let mut reader = self.readers().acquire().await?;
        count_values(&mut reader, columns).await
    }

    /// Deletes the `vault_meta` row when no column of `columns` holds a value: [`VaultRemoval::Absent`]
    /// without a row, [`VaultRemoval::InUse`] while a secret is stored. The check and the delete
    /// are one `BEGIN IMMEDIATE` transaction, so no secret is written in between.
    ///
    /// # Errors
    ///
    /// A bad identifier in `columns`, or the write fails.
    pub async fn remove_vault_meta(&self, columns: &[SecretColumn]) -> Result<VaultRemoval> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM vault_meta WHERE id = 1")
            .fetch_one(&mut *tx)
            .await?;
        if rows == 0 {
            return Ok(VaultRemoval::Absent);
        }
        let secrets = count_values(&mut tx, columns).await?;
        if secrets > 0 {
            return Ok(VaultRemoval::InUse { secrets });
        }
        sqlx::query("DELETE FROM vault_meta WHERE id = 1")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(VaultRemoval::Removed)
    }
}

/// Copies the WAL into the file and truncates it, after a password change: the pages that held
/// the old values are overwritten (`secure_delete` zeroed what moved) and the WAL keeps no copy.
/// The change is committed either way; a checkpoint another process's open read holds up is only
/// reported - the next one completes it.
async fn checkpoint(writer: &mut SqliteConnection) {
    let done: std::result::Result<(i64, i64, i64), _> =
        sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(&mut *writer)
            .await;
    match done {
        Ok((0, _, _)) => {}
        Ok(_) => {
            tracing::warn!("the WAL could not be checkpointed fully after the password change")
        }
        Err(error) => {
            tracing::warn!(%error, "the WAL cannot be checkpointed after the password change")
        }
    }
}

/// The non-null values of `columns`, counted on `connection`.
async fn count_values(connection: &mut SqliteConnection, columns: &[SecretColumn]) -> Result<u64> {
    let mut total = 0_u64;
    for column in columns {
        let table = checked_identifier(column.table)?;
        let name = checked_identifier(column.column)?;
        let count: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT count(*) FROM \"{table}\" WHERE \"{name}\" IS NOT NULL"
        )))
        .fetch_one(&mut *connection)
        .await?;
        total += u64::try_from(count).unwrap_or_default();
    }
    Ok(total)
}

/// `name` when it is a plain SQL identifier, which makes quoting it in a statement safe.
fn checked_identifier(name: &str) -> Result<&str> {
    let mut chars = name.chars();
    let plain = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain {
        Ok(name)
    } else {
        Err(DbError::BadIdentifier {
            name: name.to_owned(),
        })
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;
