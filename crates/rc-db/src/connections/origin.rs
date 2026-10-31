//! Where a connection came from, for the export and import of settings and connections
//! (RC-080-19): every connection's `import_source` and `import_key` (migration 0007), a
//! connection created from an export together with that mark in one transaction, and this
//! installation's id, which names the connections made here in an export.
//!
//! A connection keeps its mark when it is exported again, so a connection that travels from one
//! machine to another and back is recognised on every one of them: a second import updates it
//! instead of adding it twice.

use sqlx::Connection as _;

use crate::{
    Db, DbError, Result,
    connections::{
        Connection, ConnectionDraft, ConnectionId, FolderId, FtpDataMode, FtpEncoding, TreeResult,
        fetch,
        folders::require_folder,
        store::{next_sort_order, require_free_name},
        validate::Fields,
        write_error,
    },
};

/// The settings key of this installation's id: no schema row, never shown, never exported.
pub const INSTALLATION_KEY: &str = "exchange.installation";

/// A connection's mark: the source it was imported from and its key there; both `None` for a
/// connection made by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionOrigin {
    pub id: ConnectionId,
    pub source: Option<String>,
    pub key: Option<String>,
}

/// Whether `draft` would be stored: the checks of [`Db::create_connection`] without the tree's
/// (the name taken in a folder is the write's to find).
///
/// # Errors
///
/// The field the validation refuses.
pub fn check_draft(draft: &ConnectionDraft) -> TreeResult<()> {
    Fields::from_draft(draft).map(|_| ())
}

impl Db {
    /// Every connection's mark, by id.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn connection_origins(&self) -> TreeResult<Vec<ConnectionOrigin>> {
        let rows: Vec<(i64, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT id, import_source, import_key FROM connections ORDER BY id")
                .fetch_all(self.readers())
                .await?;
        Ok(rows
            .into_iter()
            .map(|(id, source, key)| ConnectionOrigin {
                id: ConnectionId(id),
                source,
                key,
            })
            .collect())
    }

    /// Adds a connection to `folder` (the root when `None`) as [`Db::create_connection`] does,
    /// marked with `source` and `key` in the same transaction.
    ///
    /// # Errors
    ///
    /// An invalid field, the folder missing, the name taken in the folder (`name_taken`, also
    /// for a mark another connection has), or the write fails.
    pub async fn create_marked_connection(
        &self,
        folder: Option<FolderId>,
        draft: &ConnectionDraft,
        source: &str,
        key: &str,
    ) -> TreeResult<Connection> {
        let fields = Fields::from_draft(draft)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(folder) = folder {
            require_folder(&mut tx, folder).await?;
        }
        require_free_name(&mut tx, folder, &fields.name, None).await?;
        let sort_order = next_sort_order(&mut tx, folder).await?;
        let done = sqlx::query(
            "INSERT INTO connections (folder_id, name, kind, host, port, username, auth_method, \
             key_path, default_view, start_directory, notes, ftp_data_mode, ftp_encoding, \
             theme_light, theme_dark, sort_order, import_source, import_key) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(folder.map(|folder| folder.0))
        .bind(&fields.name)
        .bind(fields.kind.as_str())
        .bind(&fields.host)
        .bind(i64::from(fields.port))
        .bind(&fields.username)
        .bind(fields.auth_method.as_str())
        .bind(&fields.key_path)
        .bind(fields.default_view.as_str())
        .bind(&fields.start_directory)
        .bind(&fields.notes)
        .bind(fields.ftp_data_mode.map(FtpDataMode::as_str))
        .bind(fields.ftp_encoding.map(FtpEncoding::as_str))
        .bind(&fields.theme_light)
        .bind(&fields.theme_dark)
        .bind(sort_order)
        .bind(source)
        .bind(key)
        .execute(&mut *tx)
        .await
        .map_err(|error| write_error(error, &fields.name))?;
        let connection = fetch(&mut tx, ConnectionId(done.last_insert_rowid())).await?;
        tx.commit().await?;
        Ok(connection)
    }

    /// This installation's id: 32 lower-case hex digits drawn once by SQLite and kept under
    /// [`INSTALLATION_KEY`]. A copy of the database (a backup restored elsewhere) shares it.
    ///
    /// # Errors
    ///
    /// The write or the read fails, or the stored value is not a string.
    pub async fn installation_id(&self) -> Result<String> {
        let mut writer = self.writer().await;
        sqlx::query(
            "INSERT INTO settings (key, value) \
             VALUES (?, json_quote(lower(hex(randomblob(16))))) ON CONFLICT (key) DO NOTHING",
        )
        .bind(INSTALLATION_KEY)
        .execute(&mut *writer)
        .await?;
        let stored: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(INSTALLATION_KEY)
            .fetch_one(&mut *writer)
            .await?;
        serde_json::from_str(&stored).map_err(|source| DbError::Json {
            key: INSTALLATION_KEY.to_owned(),
            source,
        })
    }
}

#[cfg(test)]
#[path = "origin_tests.rs"]
mod tests;
