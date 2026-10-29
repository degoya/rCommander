//! What a person decided about the installed plugins, and what their calls did: the tables of
//! migration 0016 (RC-100-09, `docs/plugins.md` "Installation and lifecycle").
//!
//! Like `known_hosts.rs`, this layer stores and never decides: whether a package verifies is
//! `rc-plugin-host`'s, which run is the engine's. `plugins`, `plugin_keys` and
//! `plugin_digest_revocations` are kept ([`crate::DURABLE_TABLES`]); `plugin_executions` grows
//! with every call and is cut by the shared sweep ([`crate::RETAINED_TABLES`]). A confirmed key
//! is pinned: [`Db::trust_plugin_key`] never replaces the key a `key_id` was confirmed for.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{Db, Result};

/// One installed plugin's switch and version choices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct StoredPlugin {
    pub id: String,
    pub enabled: bool,
    /// The version a person chose to run (a rollback); `None` for the newest.
    pub active_version: Option<String>,
    /// A version under test, never run as the default.
    pub staged_version: Option<String>,
    /// An update installed beside a running version: it runs from the next start.
    pub pending_version: Option<String>,
    /// It came with rCommander.
    pub bundled: bool,
    pub installed_at: String,
    pub updated_at: String,
}

/// A signing key a person confirmed, or revoked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct StoredPluginKey {
    pub key_id: String,
    /// Base64 Ed25519 public key.
    pub public_key: String,
    /// Hex SHA-256 of the key.
    pub fingerprint: String,
    pub trusted_at: String,
    /// When a person revoked it; packages it signed are refused from then on.
    pub revoked_at: Option<String>,
}

/// One finished plugin call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct PluginExecution {
    pub id: i64,
    pub recorded_at: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub plugin_type: String,
    /// The export called: `open`, `text`, ...
    pub operation: String,
    /// `ok`, `failed`, `crash`, `timeout`, `fuel`, `memory`, `denied`, `cancelled`, `refused`,
    /// `host_error`.
    pub outcome: String,
    /// The failure's stable code; never a message the plugin wrote.
    pub code: Option<String>,
    pub duration_ms: i64,
}

/// A call to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPluginExecution {
    pub plugin_id: String,
    pub plugin_version: String,
    pub plugin_type: String,
    pub operation: String,
    pub outcome: String,
    pub code: Option<String>,
    pub duration_ms: i64,
}

/// The most execution records one read returns.
pub const PLUGIN_EXECUTIONS_MAX_PAGE: u32 = 500;

impl Db {
    /// Every installed plugin's row, by id.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn plugins(&self) -> Result<Vec<StoredPlugin>> {
        Ok(sqlx::query_as(
            "SELECT id, enabled, active_version, staged_version, pending_version, bundled, \
             installed_at, updated_at FROM plugins ORDER BY id",
        )
        .fetch_all(self.readers())
        .await?)
    }

    /// The row of plugin `id`, if it has one.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn plugin(&self, id: &str) -> Result<Option<StoredPlugin>> {
        Ok(sqlx::query_as(
            "SELECT id, enabled, active_version, staged_version, pending_version, bundled, \
             installed_at, updated_at FROM plugins WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.readers())
        .await?)
    }

    /// Records an installed plugin: a new row switched on, or the row it has - a reinstall keeps
    /// what the person decided. `bundled` is set, never cleared.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn record_plugin(&self, id: &str, bundled: bool) -> Result<StoredPlugin> {
        let mut writer = self.writer().await;
        sqlx::query(
            "INSERT INTO plugins (id, bundled) VALUES (?, ?) ON CONFLICT (id) DO UPDATE SET \
             bundled = max(bundled, excluded.bundled)",
        )
        .bind(id)
        .bind(bundled)
        .execute(&mut *writer)
        .await?;
        Ok(sqlx::query_as(
            "SELECT id, enabled, active_version, staged_version, pending_version, bundled, \
             installed_at, updated_at FROM plugins WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&mut *writer)
        .await?)
    }

    /// Switches plugin `id` on or off; `false` when it has no row.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn set_plugin_enabled(&self, id: &str, enabled: bool) -> Result<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query(
            "UPDATE plugins SET enabled = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE id = ?",
        )
        .bind(enabled)
        .bind(id)
        .execute(&mut *writer)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Stores plugin `id`'s version choices whole; `false` when it has no row.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn set_plugin_versions(
        &self,
        id: &str,
        active: Option<&str>,
        staged: Option<&str>,
        pending: Option<&str>,
    ) -> Result<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query(
            "UPDATE plugins SET active_version = ?, staged_version = ?, pending_version = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(active)
        .bind(staged)
        .bind(pending)
        .bind(id)
        .execute(&mut *writer)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Forgets plugin `id`, once its last version is gone; whether it had a row.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn remove_plugin(&self, id: &str) -> Result<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query("DELETE FROM plugins WHERE id = ?")
            .bind(id)
            .execute(&mut *writer)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Every confirmed or revoked signing key, by key id.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn plugin_keys(&self) -> Result<Vec<StoredPluginKey>> {
        Ok(sqlx::query_as(
            "SELECT key_id, public_key, fingerprint, trusted_at, revoked_at FROM plugin_keys \
             ORDER BY key_id",
        )
        .fetch_all(self.readers())
        .await?)
    }

    /// Stores a key a person confirmed on first use; `false` when `key_id` is stored already -
    /// a `key_id` stays pinned to the key it was confirmed for.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn trust_plugin_key(
        &self,
        key_id: &str,
        public_key: &str,
        fingerprint: &str,
    ) -> Result<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query(
            "INSERT INTO plugin_keys (key_id, public_key, fingerprint) VALUES (?, ?, ?) \
             ON CONFLICT DO NOTHING",
        )
        .bind(key_id)
        .bind(public_key)
        .bind(fingerprint)
        .execute(&mut *writer)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Revokes the confirmed key with `fingerprint`; `false` when no such key is stored or it was
    /// revoked already.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn revoke_plugin_key(&self, fingerprint: &str) -> Result<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query(
            "UPDATE plugin_keys SET revoked_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             WHERE fingerprint = ? AND revoked_at IS NULL",
        )
        .bind(fingerprint)
        .execute(&mut *writer)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The digests of every withdrawn package version, in hex.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn plugin_digest_revocations(&self) -> Result<Vec<String>> {
        Ok(
            sqlx::query_scalar("SELECT digest FROM plugin_digest_revocations ORDER BY digest")
                .fetch_all(self.readers())
                .await?,
        )
    }

    /// Withdraws one package version by its digest; `false` when it was withdrawn already.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn revoke_plugin_digest(&self, digest: &str, reason: Option<&str>) -> Result<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query(
            "INSERT INTO plugin_digest_revocations (digest, reason) VALUES (?, ?) \
             ON CONFLICT DO NOTHING",
        )
        .bind(digest)
        .bind(reason)
        .execute(&mut *writer)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Records one finished plugin call.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn record_plugin_execution(&self, call: &NewPluginExecution) -> Result<()> {
        let mut writer = self.writer().await;
        sqlx::query(
            "INSERT INTO plugin_executions (plugin_id, plugin_version, plugin_type, operation, \
             outcome, code, duration_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&call.plugin_id)
        .bind(&call.plugin_version)
        .bind(&call.plugin_type)
        .bind(&call.operation)
        .bind(&call.outcome)
        .bind(call.code.as_deref())
        .bind(call.duration_ms.max(0))
        .execute(&mut *writer)
        .await?;
        Ok(())
    }

    /// The latest `limit` calls of plugin `id` (at most [`PLUGIN_EXECUTIONS_MAX_PAGE`]), newest
    /// first.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn plugin_executions(&self, id: &str, limit: u32) -> Result<Vec<PluginExecution>> {
        Ok(sqlx::query_as(
            "SELECT id, recorded_at, plugin_id, plugin_version, plugin_type, operation, outcome, \
             code, duration_ms FROM plugin_executions WHERE plugin_id = ? ORDER BY id DESC LIMIT ?",
        )
        .bind(id)
        .bind(i64::from(limit.min(PLUGIN_EXECUTIONS_MAX_PAGE)))
        .fetch_all(self.readers())
        .await?)
    }
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
