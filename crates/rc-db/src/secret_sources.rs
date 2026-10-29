//! Where a connection's secret lives when a source holds it instead of the vault: the
//! `secret_sources` table (migration 0017, RC-100-34).
//!
//! Only the reference is stored - the source (`plugin:<id>` or `wincred`), the entry's id in it
//! and a label to show - never a value. Which source answers and how is the engine's.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{
    Db, SecretKind,
    connections::{ConnectionError, ConnectionId, TreeResult},
};

/// The longest entry id a reference keeps, in characters (the source's own limit).
pub const MAX_SOURCE_ENTRY_ID_CHARS: usize = 256;

/// The longest label a reference keeps, in characters.
pub const MAX_SOURCE_LABEL_CHARS: usize = 512;

/// One secret of one connection, taken from a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretSourceRef {
    pub connection_id: ConnectionId,
    pub kind: SecretKind,
    /// `plugin:<id>` or `wincred`.
    pub source: String,
    /// The entry's id in the source, as its search handed it out.
    pub entry_id: String,
    /// What the form shows: title and user, never a value.
    pub entry_label: String,
}

#[derive(FromRow)]
struct Row {
    connection_id: i64,
    kind: String,
    source: String,
    entry_id: String,
    entry_label: String,
}

impl Row {
    fn into_ref(self) -> Option<SecretSourceRef> {
        let kind = SecretKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == self.kind)?;
        Some(SecretSourceRef {
            connection_id: ConnectionId(self.connection_id),
            kind,
            source: self.source,
            entry_id: self.entry_id,
            entry_label: self.entry_label,
        })
    }
}

impl Db {
    /// The references of connection `id`, by kind.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn secret_sources(&self, id: ConnectionId) -> TreeResult<Vec<SecretSourceRef>> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT connection_id, kind, source, entry_id, entry_label FROM secret_sources \
             WHERE connection_id = ? ORDER BY kind",
        )
        .bind(id.0)
        .fetch_all(self.readers())
        .await?;
        Ok(rows.into_iter().filter_map(Row::into_ref).collect())
    }

    /// The reference of the secret `kind` of connection `id`, if one is set.
    ///
    /// # Errors
    ///
    /// The query fails.
    pub async fn secret_source(
        &self,
        id: ConnectionId,
        kind: SecretKind,
    ) -> TreeResult<Option<SecretSourceRef>> {
        let row: Option<Row> = sqlx::query_as(
            "SELECT connection_id, kind, source, entry_id, entry_label FROM secret_sources \
             WHERE connection_id = ? AND kind = ?",
        )
        .bind(id.0)
        .bind(kind.as_str())
        .fetch_optional(self.readers())
        .await?;
        Ok(row.and_then(Row::into_ref))
    }

    /// Sets the reference, replacing one of the same connection and kind.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ConnectionNotFound`]; the table's checks refuse the source, the id or
    /// the label (the engine checks them first); or the write fails.
    pub async fn set_secret_source(&self, reference: &SecretSourceRef) -> TreeResult<()> {
        let mut writer = self.writer().await;
        let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM connections WHERE id = ?")
            .bind(reference.connection_id.0)
            .fetch_optional(&mut *writer)
            .await?;
        if exists.is_none() {
            return Err(ConnectionError::ConnectionNotFound {
                id: reference.connection_id,
            });
        }
        sqlx::query(
            "INSERT INTO secret_sources (connection_id, kind, source, entry_id, entry_label) \
             VALUES (?, ?, ?, ?, ?) ON CONFLICT (connection_id, kind) DO UPDATE SET \
             source = excluded.source, entry_id = excluded.entry_id, \
             entry_label = excluded.entry_label, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        )
        .bind(reference.connection_id.0)
        .bind(reference.kind.as_str())
        .bind(&reference.source)
        .bind(&reference.entry_id)
        .bind(&reference.entry_label)
        .execute(&mut *writer)
        .await?;
        Ok(())
    }

    /// Removes the reference of the secret `kind` of connection `id`; whether one was set.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn remove_secret_source(
        &self,
        id: ConnectionId,
        kind: SecretKind,
    ) -> TreeResult<bool> {
        let mut writer = self.writer().await;
        let done = sqlx::query("DELETE FROM secret_sources WHERE connection_id = ? AND kind = ?")
            .bind(id.0)
            .bind(kind.as_str())
            .execute(&mut *writer)
            .await?;
        Ok(done.rows_affected() > 0)
    }
}

#[cfg(test)]
#[path = "secret_sources_tests.rs"]
mod tests;
