//! Connections: read, create, change, move, delete.

use sqlx::{Connection as _, SqliteConnection};

use crate::{
    Db,
    connections::{
        Connection, ConnectionDraft, ConnectionError, ConnectionId, ConnectionPatch, EndpointRule,
        FolderId, FtpDataMode, FtpEncoding, TreeResult, connection_from_row,
        endpoint::guard_endpoint, folders::require_folder, select_connections, validate::Fields,
        write_error,
    },
};

impl Db {
    /// The connection `id`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ConnectionNotFound`], or the read fails.
    pub async fn connection(&self, id: ConnectionId) -> TreeResult<Connection> {
        let row = sqlx::query(select_connections!("WHERE id = ?"))
            .bind(id.0)
            .fetch_optional(self.readers())
            .await?
            .ok_or(ConnectionError::ConnectionNotFound { id })?;
        Ok(connection_from_row(&row)?)
    }

    /// The connections named `name` in any folder, ignoring ASCII case, oldest first: how a
    /// location such as `ftp://<name>/...` finds its connection.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn connections_named(&self, name: &str) -> TreeResult<Vec<Connection>> {
        let rows = sqlx::query(select_connections!(
            "WHERE name = ? COLLATE NOCASE ORDER BY id"
        ))
        .bind(name.trim())
        .fetch_all(self.readers())
        .await?;
        Ok(rows
            .iter()
            .map(connection_from_row)
            .collect::<sqlx::Result<Vec<_>>>()?)
    }

    /// Adds a connection to `folder` (the root when `None`), after its folder's last one.
    ///
    /// # Errors
    ///
    /// An invalid field, the folder missing, the name taken in the folder, or the write fails.
    pub async fn create_connection(
        &self,
        folder: Option<FolderId>,
        draft: &ConnectionDraft,
    ) -> TreeResult<Connection> {
        let fields = Fields::from_draft(draft)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let id = insert(&mut tx, folder, &fields).await?;
        let connection = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(connection)
    }

    /// Changes the fields `patch` names; the connection stays in its folder.
    ///
    /// # Errors
    ///
    /// The connection missing, an invalid field, the new name taken, or the write fails.
    pub async fn update_connection(
        &self,
        id: ConnectionId,
        patch: &ConnectionPatch,
    ) -> TreeResult<Connection> {
        self.update_connection_with(id, patch, EndpointRule::Free)
            .await
    }

    /// [`Db::update_connection`] under `rule`, judged inside the change's transaction: with
    /// [`EndpointRule::KeepWhileSecretsHeld`] a change of the host, port, kind or user of a
    /// connection that holds a secret is refused (RC-080-50).
    ///
    /// # Errors
    ///
    /// As [`Db::update_connection`], and [`ConnectionError::EndpointChangeNeedsGui`].
    pub async fn update_connection_with(
        &self,
        id: ConnectionId,
        patch: &ConnectionPatch,
        rule: EndpointRule,
    ) -> TreeResult<Connection> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        if rule != EndpointRule::Free {
            let current = fetch(&mut tx, id).await?;
            guard_endpoint(&mut tx, &current, patch, rule).await?;
        }
        let connection = apply_patch(&mut tx, id, patch).await?;
        tx.commit().await?;
        Ok(connection)
    }

    /// Moves a connection into `folder` (the root when `None`): at `sort_order`, or after the
    /// folder's last connection. Moving within its own folder reorders it.
    ///
    /// # Errors
    ///
    /// The connection or folder missing, its name taken in the folder, or the write fails.
    pub async fn move_connection(
        &self,
        id: ConnectionId,
        folder: Option<FolderId>,
        sort_order: Option<i64>,
    ) -> TreeResult<Connection> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let current = fetch(&mut tx, id).await?;
        if let Some(folder) = folder {
            require_folder(&mut tx, folder).await?;
        }
        require_free_name(&mut tx, folder, &current.name, Some(id)).await?;
        let sort_order = match sort_order {
            Some(order) => order,
            None => next_sort_order(&mut tx, folder).await?,
        };
        sqlx::query(
            "UPDATE connections SET folder_id = ?, sort_order = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(folder.map(|folder| folder.0))
        .bind(sort_order)
        .bind(id.0)
        .execute(&mut *tx)
        .await
        .map_err(|error| write_error(error, &current.name))?;
        let connection = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(connection)
    }

    /// Deletes a connection.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::ConnectionNotFound`], or the write fails.
    pub async fn delete_connection(&self, id: ConnectionId) -> TreeResult<()> {
        let mut writer = self.writer().await;
        let done = sqlx::query("DELETE FROM connections WHERE id = ?")
            .bind(id.0)
            .execute(&mut *writer)
            .await?;
        if done.rows_affected() == 0 {
            return Err(ConnectionError::ConnectionNotFound { id });
        }
        Ok(())
    }
}

/// Adds a connection with the checked `fields` to `folder`, after its last one; its new id.
pub(super) async fn insert(
    connection: &mut SqliteConnection,
    folder: Option<FolderId>,
    fields: &Fields,
) -> TreeResult<ConnectionId> {
    if let Some(folder) = folder {
        require_folder(connection, folder).await?;
    }
    require_free_name(connection, folder, &fields.name, None).await?;
    let sort_order = next_sort_order(connection, folder).await?;
    let done = sqlx::query(
        "INSERT INTO connections (folder_id, name, kind, host, port, username, auth_method, \
         key_path, default_view, start_directory, notes, ftp_data_mode, ftp_encoding, \
         sort_order) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(sort_order)
    .execute(&mut *connection)
    .await
    .map_err(|error| write_error(error, &fields.name))?;
    Ok(ConnectionId(done.last_insert_rowid()))
}

/// Changes the fields `patch` names on the writer inside its transaction; the connection as
/// it is then.
pub(super) async fn apply_patch(
    connection: &mut SqliteConnection,
    id: ConnectionId,
    patch: &ConnectionPatch,
) -> TreeResult<Connection> {
    let current = fetch(connection, id).await?;
    let fields = Fields::patched(&current, patch)?;
    require_free_name(connection, current.folder_id, &fields.name, Some(id)).await?;
    sqlx::query(
        "UPDATE connections SET name = ?, kind = ?, host = ?, port = ?, username = ?, \
         auth_method = ?, key_path = ?, default_view = ?, start_directory = ?, notes = ?, \
         ftp_data_mode = ?, ftp_encoding = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
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
    .bind(id.0)
    .execute(&mut *connection)
    .await
    .map_err(|error| write_error(error, &fields.name))?;
    fetch(connection, id).await
}

/// The connection `id`, read on the writer inside its transaction.
pub(crate) async fn fetch(
    connection: &mut SqliteConnection,
    id: ConnectionId,
) -> TreeResult<Connection> {
    let row = sqlx::query(select_connections!("WHERE id = ?"))
        .bind(id.0)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(ConnectionError::ConnectionNotFound { id })?;
    Ok(connection_from_row(&row)?)
}

/// Fails with [`ConnectionError::NameTaken`] when another connection of `folder` (other than
/// `except`) has `name`, ignoring ASCII case like the unique index.
pub(super) async fn require_free_name(
    connection: &mut SqliteConnection,
    folder: Option<FolderId>,
    name: &str,
    except: Option<ConnectionId>,
) -> TreeResult<()> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM connections WHERE ifnull(folder_id, 0) = ifnull(?, 0) \
         AND name = ? COLLATE NOCASE AND id <> ?)",
    )
    .bind(folder.map(|folder| folder.0))
    .bind(name)
    .bind(except.map_or(0, |id| id.0))
    .fetch_one(&mut *connection)
    .await?;
    if taken {
        return Err(ConnectionError::NameTaken {
            name: name.to_owned(),
        });
    }
    Ok(())
}

/// One past the largest `sort_order` in `folder`; 0 in an empty one.
pub(super) async fn next_sort_order(
    connection: &mut SqliteConnection,
    folder: Option<FolderId>,
) -> TreeResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT ifnull(max(sort_order) + 1, 0) FROM connections \
         WHERE ifnull(folder_id, 0) = ifnull(?, 0)",
    )
    .bind(folder.map(|folder| folder.0))
    .fetch_one(&mut *connection)
    .await?)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
