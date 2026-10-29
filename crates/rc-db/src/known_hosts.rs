//! The SSH host-key trust store: the `known_hosts` table (migration 0004).
//!
//! This layer stores and never decides. Whether an offered key is trusted, unknown or changed
//! is `rc-remote`'s verdict (it also has to check hashed names by HMAC); what it asks for here
//! is the rows that may belong to a host ([`Db::known_host_candidates`]). Trusting a first
//! sighting ([`Db::add_known_host`]) never overwrites a stored key: replacing one is its own
//! call ([`Db::replace_known_host`]) that names the key it replaces.

use serde::{Deserialize, Serialize};
use sqlx::{Connection, FromRow};

use crate::{Db, Result};

/// The columns of a row, as a literal: sqlx 0.9 takes `&'static str` SQL, so the statements
/// are put together with `concat!`.
macro_rules! columns {
    () => {
        "id, host, port, algorithm, public_key, fingerprint_sha256, added_at, last_seen_at, source"
    };
}

/// The insert that never overwrites: a stored host, port and key type keeps its row.
const INSERT_NEW: &str = concat!(
    "INSERT INTO known_hosts (host, port, algorithm, public_key, fingerprint_sha256, source) ",
    "VALUES (?, ?, ?, ?, ?, ?) ",
    "ON CONFLICT (host, port, algorithm) DO NOTHING RETURNING ",
    columns!()
);

/// The insert of an explicit replacement, which may overwrite.
const INSERT_REPLACING: &str = concat!(
    "INSERT INTO known_hosts (host, port, algorithm, public_key, fingerprint_sha256, source) ",
    "VALUES (?, ?, ?, ?, ?, ?) ",
    "ON CONFLICT (host, port, algorithm) DO UPDATE SET public_key = excluded.public_key, ",
    "fingerprint_sha256 = excluded.fingerprint_sha256, source = excluded.source, ",
    "added_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), last_seen_at = NULL RETURNING ",
    columns!()
);

/// What a multi-statement write begins with: the write lock at once, not at the first write.
const BEGIN_IMMEDIATE: &str = "BEGIN IMMEDIATE";

/// The prefix of a hashed `known_hosts` name.
pub const HASHED_PREFIX: &str = "|1|";

/// Where a stored key came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownHostSource {
    /// A person confirmed it in the app.
    User,
    /// Read from an OpenSSH `known_hosts` file.
    Import,
}

impl KnownHostSource {
    /// The value of the `source` column.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Import => "import",
        }
    }
}

/// One stored host key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownHost {
    pub id: i64,
    /// The host name as typed (lower case), or a hashed name `|1|salt|hash`.
    pub host: String,
    /// The port; 0 for a hashed name, whose port is inside the hash.
    pub port: u16,
    /// The key type as SSH names it (`ssh-ed25519`, `ecdsa-sha2-nistp256`, `ssh-rsa`, ...).
    pub algorithm: String,
    /// The key's wire blob in base64, as `known_hosts` spells it.
    pub public_key: String,
    /// `SHA256:<base64>` as `ssh-keygen -lf` prints it.
    pub fingerprint_sha256: String,
    /// When it was stored (UTC, ISO 8601).
    pub added_at: String,
    /// When a connection last presented it; `None` before the first one.
    pub last_seen_at: Option<String>,
    pub source: KnownHostSource,
}

impl KnownHost {
    /// Whether the host name is hashed (`|1|salt|hash`).
    #[must_use]
    pub fn is_hashed(&self) -> bool {
        self.host.starts_with(HASHED_PREFIX)
    }
}

/// A key to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewKnownHost {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint_sha256: String,
    pub source: KnownHostSource,
}

/// What an import stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownHostsImport {
    /// New rows.
    pub added: usize,
    /// Entries already stored with exactly this key.
    pub unchanged: usize,
    /// Entries whose host, port and key type are stored with a different key: left as they are,
    /// since an import never replaces a key a person may have confirmed.
    pub conflicts: Vec<NewKnownHost>,
}

#[derive(FromRow)]
struct KnownHostRow {
    id: i64,
    host: String,
    port: u16,
    algorithm: String,
    public_key: String,
    fingerprint_sha256: String,
    added_at: String,
    last_seen_at: Option<String>,
    source: String,
}

impl From<KnownHostRow> for KnownHost {
    fn from(row: KnownHostRow) -> Self {
        Self {
            id: row.id,
            host: row.host,
            port: row.port,
            algorithm: row.algorithm,
            public_key: row.public_key,
            fingerprint_sha256: row.fingerprint_sha256,
            added_at: row.added_at,
            last_seen_at: row.last_seen_at,
            // The table's CHECK constraint allows these two values only.
            source: if row.source == KnownHostSource::User.as_str() {
                KnownHostSource::User
            } else {
                KnownHostSource::Import
            },
        }
    }
}

impl Db {
    /// Every stored key, by host, port and key type.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn known_hosts(&self) -> Result<Vec<KnownHost>> {
        let rows: Vec<KnownHostRow> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM known_hosts ORDER BY host, port, algorithm"
        ))
        .fetch_all(self.readers())
        .await?;
        Ok(rows.into_iter().map(KnownHost::from).collect())
    }

    /// The stored key with this id.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn known_host(&self, id: i64) -> Result<Option<KnownHost>> {
        let row: Option<KnownHostRow> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM known_hosts WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(self.readers())
        .await?;
        Ok(row.map(KnownHost::from))
    }

    /// The rows that may belong to `host` on `port`: its plain rows and every hashed one, which
    /// only the caller can match (HMAC-SHA1 over the name). `host` is compared as given; the
    /// caller lower-cases it like the stored names.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn known_host_candidates(&self, host: &str, port: u16) -> Result<Vec<KnownHost>> {
        let rows: Vec<KnownHostRow> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM known_hosts WHERE (host = ? AND port = ?) OR substr(host, 1, 3) = ? ORDER BY id"
        ))
        .bind(host)
        .bind(i64::from(port))
        .bind(HASHED_PREFIX)
        .fetch_all(self.readers())
        .await?;
        Ok(rows.into_iter().map(KnownHost::from).collect())
    }

    /// Stores a first sighting. `None` when the host, port and key type are stored already -
    /// with this key or another: this call never replaces a key ([`Db::replace_known_host`]
    /// does).
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn add_known_host(&self, key: &NewKnownHost) -> Result<Option<KnownHost>> {
        let mut writer = self.writer().await;
        let row: Option<KnownHostRow> = sqlx::query_as(INSERT_NEW)
            .bind(&key.host)
            .bind(i64::from(key.port))
            .bind(&key.algorithm)
            .bind(&key.public_key)
            .bind(&key.fingerprint_sha256)
            .bind(key.source.as_str())
            .fetch_optional(&mut *writer)
            .await?;
        Ok(row.map(KnownHost::from))
    }

    /// Replaces the stored key `id` with `key`, but only while `id` still carries
    /// `expected_fingerprint`: a decision made on what a person saw cannot replace a key that
    /// changed since. `None` when it no longer does (or `id` is gone); nothing changes then.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn replace_known_host(
        &self,
        id: i64,
        expected_fingerprint: &str,
        key: &NewKnownHost,
    ) -> Result<Option<KnownHost>> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with(BEGIN_IMMEDIATE).await?;
        let removed =
            sqlx::query("DELETE FROM known_hosts WHERE id = ? AND fingerprint_sha256 = ?")
                .bind(id)
                .bind(expected_fingerprint)
                .execute(&mut *tx)
                .await?;
        if removed.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(None);
        }
        let row: KnownHostRow = sqlx::query_as(INSERT_REPLACING)
            .bind(&key.host)
            .bind(i64::from(key.port))
            .bind(&key.algorithm)
            .bind(&key.public_key)
            .bind(&key.fingerprint_sha256)
            .bind(key.source.as_str())
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(row.into()))
    }

    /// Notes that a connection presented the key `id` just now.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn touch_known_host(&self, id: i64) -> Result<()> {
        let mut writer = self.writer().await;
        sqlx::query(
            "UPDATE known_hosts SET last_seen_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE id = ?",
        )
        .bind(id)
        .execute(&mut *writer)
        .await?;
        Ok(())
    }

    /// Removes the key `id`; answers the row it removed, `None` when there was none.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn remove_known_host(&self, id: i64) -> Result<Option<KnownHost>> {
        let mut writer = self.writer().await;
        let row: Option<KnownHostRow> = sqlx::query_as(concat!(
            "DELETE FROM known_hosts WHERE id = ? RETURNING ",
            columns!()
        ))
        .bind(id)
        .fetch_optional(&mut *writer)
        .await?;
        Ok(row.map(KnownHost::from))
    }

    /// Stores imported keys in one transaction. A key already stored is counted, one stored
    /// with another key is reported and left alone.
    ///
    /// # Errors
    ///
    /// A write fails; nothing is stored then.
    pub async fn import_known_hosts(&self, keys: &[NewKnownHost]) -> Result<KnownHostsImport> {
        let mut summary = KnownHostsImport::default();
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with(BEGIN_IMMEDIATE).await?;
        for key in keys {
            let added: Option<KnownHostRow> = sqlx::query_as(INSERT_NEW)
                .bind(&key.host)
                .bind(i64::from(key.port))
                .bind(&key.algorithm)
                .bind(&key.public_key)
                .bind(&key.fingerprint_sha256)
                .bind(key.source.as_str())
                .fetch_optional(&mut *tx)
                .await?;
            if added.is_some() {
                summary.added += 1;
                continue;
            }
            let stored: Option<String> = sqlx::query_scalar(
                "SELECT public_key FROM known_hosts WHERE host = ? AND port = ? AND algorithm = ?",
            )
            .bind(&key.host)
            .bind(i64::from(key.port))
            .bind(&key.algorithm)
            .fetch_optional(&mut *tx)
            .await?;
            if stored.as_deref() == Some(key.public_key.as_str()) {
                summary.unchanged += 1;
            } else {
                summary.conflicts.push(key.clone());
            }
        }
        tx.commit().await?;
        Ok(summary)
    }
}

#[cfg(test)]
#[path = "known_hosts_tests.rs"]
mod tests;
