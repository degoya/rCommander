//! The SSH host keys marked `@revoked`: the `revoked_host_keys` table (migration 0024,
//! RC-080-67).
//!
//! Like `known_hosts.rs`, this layer stores and never decides: refusing a revoked key before
//! any trust question is `rc-remote`'s check. A key is revoked whatever the host; a person
//! removes a row in the settings, nothing else does.

use serde::{Deserialize, Serialize};
use sqlx::{Connection, FromRow};

use crate::{Db, Result};

/// The columns of a row, as a literal for `concat!` (sqlx 0.9 takes `&'static str` SQL).
macro_rules! columns {
    () => {
        "id, algorithm, public_key, fingerprint_sha256, hosts, source, added_at"
    };
}

/// One revoked key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct RevokedHostKey {
    pub id: i64,
    /// The key type as SSH names it (`ssh-ed25519`, ...).
    pub algorithm: String,
    /// The key's wire blob in base64, as `known_hosts` spells it.
    pub public_key: String,
    /// `SHA256:<base64>` as `ssh-keygen -lf` prints it.
    pub fingerprint_sha256: String,
    /// The `@revoked` line's name field as written: shown, never matched.
    pub hosts: String,
    /// The file the line came from.
    pub source: String,
    /// When it was stored (UTC, ISO 8601).
    pub added_at: String,
}

/// A revoked key to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRevokedHostKey {
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint_sha256: String,
    pub hosts: String,
    pub source: String,
}

impl Db {
    /// Every revoked key, by key type and fingerprint.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn revoked_host_keys(&self) -> Result<Vec<RevokedHostKey>> {
        let rows: Vec<RevokedHostKey> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM revoked_host_keys ORDER BY algorithm, fingerprint_sha256"
        ))
        .fetch_all(self.readers())
        .await?;
        Ok(rows)
    }

    /// The revoked key with this fingerprint, if it is one.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn revoked_host_key(&self, fingerprint: &str) -> Result<Option<RevokedHostKey>> {
        let row: Option<RevokedHostKey> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM revoked_host_keys WHERE fingerprint_sha256 = ?"
        ))
        .bind(fingerprint)
        .fetch_optional(self.readers())
        .await?;
        Ok(row)
    }

    /// Stores revoked keys in one transaction; a key revoked already keeps its row. Answers how
    /// many rows are new.
    ///
    /// # Errors
    ///
    /// A write fails; nothing is stored then.
    pub async fn add_revoked_host_keys(&self, keys: &[NewRevokedHostKey]) -> Result<usize> {
        if keys.is_empty() {
            return Ok(0);
        }
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let mut added = 0;
        for key in keys {
            let done = sqlx::query(
                "INSERT INTO revoked_host_keys (algorithm, public_key, fingerprint_sha256, hosts, \
                 source) VALUES (?, ?, ?, ?, ?) ON CONFLICT (fingerprint_sha256) DO NOTHING",
            )
            .bind(&key.algorithm)
            .bind(&key.public_key)
            .bind(&key.fingerprint_sha256)
            .bind(&key.hosts)
            .bind(&key.source)
            .execute(&mut *tx)
            .await?;
            if done.rows_affected() > 0 {
                added += 1;
            }
        }
        tx.commit().await?;
        Ok(added)
    }

    /// Removes the revoked key `id` - a person's decision; answers the row it removed, `None`
    /// when there was none.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn remove_revoked_host_key(&self, id: i64) -> Result<Option<RevokedHostKey>> {
        let mut writer = self.writer().await;
        let row: Option<RevokedHostKey> = sqlx::query_as(concat!(
            "DELETE FROM revoked_host_keys WHERE id = ? RETURNING ",
            columns!()
        ))
        .bind(id)
        .fetch_optional(&mut *writer)
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
#[path = "revoked_host_keys_tests.rs"]
mod tests;
