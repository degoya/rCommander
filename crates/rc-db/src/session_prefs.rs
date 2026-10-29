//! A connection's SSH session settings (`connection_session_prefs`, migration 0010): the order of
//! the authentication methods, the one agent key to offer, the keepalive and the idle close.
//!
//! This layer stores what the engine checked; it does not parse the order (its methods are
//! `rc-remote`'s). A connection without a row takes the engine's defaults, and its row goes with
//! it when it is deleted.

use serde::{Deserialize, Serialize};
use sqlx::{Connection as _, FromRow};

use crate::{
    Db, Result,
    connections::{ConnectionError, ConnectionId, TreeResult},
};

/// The settings as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct StoredSessionPrefs {
    /// The methods in order, comma-separated (`agent,key,password`).
    pub auth_order: String,
    /// The `SHA256:` fingerprint of the one agent key to offer; every key when `None`.
    pub agent_fingerprint: Option<String>,
    pub keepalive_secs: u32,
    /// 0: a session without channels stays open.
    pub idle_close_secs: u32,
}

impl Db {
    /// The stored settings of connection `id`; `None` when it has none (or does not exist).
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn session_prefs(&self, id: ConnectionId) -> Result<Option<StoredSessionPrefs>> {
        Ok(sqlx::query_as(
            "SELECT auth_order, agent_fingerprint, keepalive_secs, idle_close_secs \
             FROM connection_session_prefs WHERE connection_id = ?",
        )
        .bind(id.0)
        .fetch_optional(self.readers())
        .await?)
    }

    /// Stores `prefs` for connection `id`, replacing what was there.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ConnectionNotFound`], or the write fails.
    pub async fn set_session_prefs(
        &self,
        id: ConnectionId,
        prefs: &StoredSessionPrefs,
    ) -> TreeResult<()> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM connections WHERE id = ?)")
                .bind(id.0)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(ConnectionError::ConnectionNotFound { id });
        }
        sqlx::query(
            "INSERT INTO connection_session_prefs \
             (connection_id, auth_order, agent_fingerprint, keepalive_secs, idle_close_secs) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (connection_id) DO UPDATE SET auth_order = excluded.auth_order, \
             agent_fingerprint = excluded.agent_fingerprint, \
             keepalive_secs = excluded.keepalive_secs, idle_close_secs = excluded.idle_close_secs, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        )
        .bind(id.0)
        .bind(&prefs.auth_order)
        .bind(&prefs.agent_fingerprint)
        .bind(prefs.keepalive_secs)
        .bind(prefs.idle_close_secs)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "session_prefs_tests.rs"]
mod tests;
