//! Folders, and the whole tree in one read.

use std::collections::HashMap;

use sqlx::{Connection as _, SqliteConnection};

use crate::{
    Db,
    connections::{
        Connection, ConnectionError, ConnectionFolder, ConnectionTree, Credential, FolderDeletion,
        FolderId, FolderNode, MAX_DEPTH, TreeResult, connection_from_row, credentials,
        folder_from_row, select_connections, select_folders, validate, write_error,
    },
};

impl Db {
    /// The whole tree, read in one snapshot, each level in display order (`sort_order`, then
    /// name ignoring case; shared credentials by name).
    ///
    /// # Errors
    ///
    /// The read fails or a stored value is not one this build knows.
    pub async fn connection_tree(&self) -> TreeResult<ConnectionTree> {
        let mut snapshot = self.readers().begin().await?;
        let folders = sqlx::query(select_folders!(
            "ORDER BY sort_order, name COLLATE NOCASE, id"
        ))
        .fetch_all(&mut *snapshot)
        .await?
        .iter()
        .map(folder_from_row)
        .collect::<sqlx::Result<Vec<_>>>()?;
        let connections = sqlx::query(select_connections!(
            "ORDER BY sort_order, name COLLATE NOCASE, id"
        ))
        .fetch_all(&mut *snapshot)
        .await?
        .iter()
        .map(connection_from_row)
        .collect::<sqlx::Result<Vec<_>>>()?;
        let credentials = credentials::all(&mut snapshot).await?;
        snapshot.rollback().await?;
        Ok(build_tree(folders, connections, credentials))
    }

    /// The folder `id`.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::FolderNotFound`], or the read fails.
    #[cfg(test)]
    pub async fn connection_folder(&self, id: FolderId) -> TreeResult<ConnectionFolder> {
        let row = sqlx::query(select_folders!("WHERE id = ?"))
            .bind(id.0)
            .fetch_optional(self.readers())
            .await?
            .ok_or(ConnectionError::FolderNotFound { id })?;
        Ok(folder_from_row(&row)?)
    }

    /// Adds a folder to `parent` (the root when `None`), after its last folder.
    ///
    /// # Errors
    ///
    /// A blank name, the parent missing, the name taken among its folders, nesting deeper than
    /// [`MAX_DEPTH`], or the write fails.
    pub async fn create_folder(
        &self,
        parent: Option<FolderId>,
        name: &str,
    ) -> TreeResult<ConnectionFolder> {
        let name = validate::name(name)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        if depth(&mut tx, parent).await? + 1 > MAX_DEPTH {
            return Err(ConnectionError::FolderTooDeep { max: MAX_DEPTH });
        }
        require_free_name(&mut tx, parent, &name, None).await?;
        let sort_order = next_sort_order(&mut tx, parent).await?;
        let done = sqlx::query(
            "INSERT INTO connection_folders (parent_id, name, sort_order) VALUES (?, ?, ?)",
        )
        .bind(parent.map(|parent| parent.0))
        .bind(&name)
        .bind(sort_order)
        .execute(&mut *tx)
        .await
        .map_err(|error| write_error(error, &name))?;
        let folder = fetch(&mut tx, FolderId(done.last_insert_rowid())).await?;
        tx.commit().await?;
        Ok(folder)
    }

    /// Gives a folder another name.
    ///
    /// # Errors
    ///
    /// A blank name, the folder missing, the name taken among its siblings, or the write fails.
    pub async fn rename_folder(&self, id: FolderId, name: &str) -> TreeResult<ConnectionFolder> {
        let name = validate::name(name)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let current = fetch(&mut tx, id).await?;
        require_free_name(&mut tx, current.parent_id, &name, Some(id)).await?;
        sqlx::query(
            "UPDATE connection_folders SET name = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(&name)
        .bind(id.0)
        .execute(&mut *tx)
        .await
        .map_err(|error| write_error(error, &name))?;
        let folder = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(folder)
    }

    /// Moves a folder, with everything in it, into `parent` (the root when `None`): at
    /// `sort_order`, or after the parent's last folder.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::FolderCycle`] when `parent` is the folder or inside it, nesting deeper
    /// than [`MAX_DEPTH`], a folder missing, the name taken in `parent`, or the write fails.
    pub async fn move_folder(
        &self,
        id: FolderId,
        parent: Option<FolderId>,
        sort_order: Option<i64>,
    ) -> TreeResult<ConnectionFolder> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let current = fetch(&mut tx, id).await?;
        let target_path = path_to_root(&mut tx, parent).await?;
        if target_path.contains(&id) {
            return Err(ConnectionError::FolderCycle);
        }
        let height = subtree(&mut tx, id).await?.height;
        if target_path.len() + height > MAX_DEPTH {
            return Err(ConnectionError::FolderTooDeep { max: MAX_DEPTH });
        }
        require_free_name(&mut tx, parent, &current.name, Some(id)).await?;
        let sort_order = match sort_order {
            Some(order) => order,
            None => next_sort_order(&mut tx, parent).await?,
        };
        sqlx::query(
            "UPDATE connection_folders SET parent_id = ?, sort_order = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(parent.map(|parent| parent.0))
        .bind(sort_order)
        .bind(id.0)
        .execute(&mut *tx)
        .await
        .map_err(|error| write_error(error, &current.name))?;
        let folder = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(folder)
    }

    /// Deletes a folder: an empty one always, one with folders, connections or shared
    /// credentials inside only when `recursive`, and then everything inside with it.
    ///
    /// # Errors
    ///
    /// [`ConnectionError::FolderNotEmpty`], the folder missing, or the write fails.
    pub async fn delete_folder(&self, id: FolderId, recursive: bool) -> TreeResult<FolderDeletion> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        fetch(&mut tx, id).await?;
        let inside = subtree(&mut tx, id).await?;
        if !recursive && (inside.folders > 1 || inside.connections > 0 || inside.credentials > 0) {
            return Err(ConnectionError::FolderNotEmpty { id });
        }
        // The foreign keys cascade to the subfolders and every connection and shared credential
        // inside.
        sqlx::query("DELETE FROM connection_folders WHERE id = ?")
            .bind(id.0)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(FolderDeletion {
            folders: inside.folders,
            connections: inside.connections,
        })
    }
}

/// Fails with [`ConnectionError::FolderNotFound`] unless the folder `id` exists.
pub(crate) async fn require_folder(
    connection: &mut SqliteConnection,
    id: FolderId,
) -> TreeResult<()> {
    fetch(connection, id).await.map(drop)
}

/// The folder `id`, read on the writer inside its transaction.
async fn fetch(connection: &mut SqliteConnection, id: FolderId) -> TreeResult<ConnectionFolder> {
    let row = sqlx::query(select_folders!("WHERE id = ?"))
        .bind(id.0)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(ConnectionError::FolderNotFound { id })?;
    Ok(folder_from_row(&row)?)
}

/// `folder` and its ancestors up to the root, nearest first; empty for the root itself.
async fn path_to_root(
    connection: &mut SqliteConnection,
    folder: Option<FolderId>,
) -> TreeResult<Vec<FolderId>> {
    let Some(folder) = folder else {
        return Ok(Vec::new());
    };
    fetch(connection, folder).await?;
    // The level bound only stops a damaged table from looping; the writes keep it acyclic.
    let ids: Vec<i64> = sqlx::query_scalar(
        "WITH RECURSIVE up (id, parent_id, level) AS ( \
             SELECT id, parent_id, 1 FROM connection_folders WHERE id = ? \
             UNION ALL \
             SELECT folder.id, folder.parent_id, up.level + 1 \
             FROM connection_folders AS folder JOIN up ON folder.id = up.parent_id \
             WHERE up.level <= ? \
         ) SELECT id FROM up ORDER BY level",
    )
    .bind(folder.0)
    .bind(i64::try_from(MAX_DEPTH).unwrap_or(i64::MAX))
    .fetch_all(&mut *connection)
    .await?;
    Ok(ids.into_iter().map(FolderId).collect())
}

/// How deep `folder` sits: 0 for the root, 1 for the root's folders.
async fn depth(connection: &mut SqliteConnection, folder: Option<FolderId>) -> TreeResult<usize> {
    Ok(path_to_root(connection, folder).await?.len())
}

/// What a folder's subtree holds.
struct Subtree {
    /// Folders, the folder itself included.
    folders: u64,
    /// Connections at any depth.
    connections: u64,
    /// Shared credentials at any depth (RC-050-12).
    credentials: u64,
    /// Levels, 1 for a folder without subfolders.
    height: usize,
}

async fn subtree(connection: &mut SqliteConnection, id: FolderId) -> TreeResult<Subtree> {
    let (folders, connections, credentials, height): (i64, i64, i64, i64) = sqlx::query_as(
        "WITH RECURSIVE down (id, level) AS ( \
             SELECT id, 1 FROM connection_folders WHERE id = ? \
             UNION ALL \
             SELECT folder.id, down.level + 1 \
             FROM connection_folders AS folder JOIN down ON folder.parent_id = down.id \
             WHERE down.level <= ? \
         ) SELECT count(*), \
             (SELECT count(*) FROM connections WHERE folder_id IN (SELECT id FROM down)), \
             (SELECT count(*) FROM credentials WHERE folder_id IN (SELECT id FROM down)), \
             ifnull(max(level), 0) \
         FROM down",
    )
    .bind(id.0)
    .bind(i64::try_from(MAX_DEPTH).unwrap_or(i64::MAX))
    .fetch_one(&mut *connection)
    .await?;
    Ok(Subtree {
        folders: u64::try_from(folders).unwrap_or(0),
        connections: u64::try_from(connections).unwrap_or(0),
        credentials: u64::try_from(credentials).unwrap_or(0),
        height: usize::try_from(height).unwrap_or(0),
    })
}

/// Fails with [`ConnectionError::NameTaken`] when another folder of `parent` (other than
/// `except`) has `name`, ignoring ASCII case like the unique index.
async fn require_free_name(
    connection: &mut SqliteConnection,
    parent: Option<FolderId>,
    name: &str,
    except: Option<FolderId>,
) -> TreeResult<()> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM connection_folders \
         WHERE ifnull(parent_id, 0) = ifnull(?, 0) AND name = ? COLLATE NOCASE AND id <> ?)",
    )
    .bind(parent.map(|parent| parent.0))
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

/// One past the largest `sort_order` among `parent`'s folders; 0 when it has none.
async fn next_sort_order(
    connection: &mut SqliteConnection,
    parent: Option<FolderId>,
) -> TreeResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT ifnull(max(sort_order) + 1, 0) FROM connection_folders \
         WHERE ifnull(parent_id, 0) = ifnull(?, 0)",
    )
    .bind(parent.map(|parent| parent.0))
    .fetch_one(&mut *connection)
    .await?)
}

/// The nested tree from flat lists already in display order. Each folder's children are taken
/// from the map once, so even a damaged table with a cycle cannot loop; a folder whose parent is
/// missing is left out.
pub(crate) fn build_tree(
    folders: Vec<ConnectionFolder>,
    connections: Vec<Connection>,
    credentials: Vec<Credential>,
) -> ConnectionTree {
    let mut child_folders: HashMap<Option<FolderId>, Vec<ConnectionFolder>> = HashMap::new();
    for folder in folders {
        child_folders
            .entry(folder.parent_id)
            .or_default()
            .push(folder);
    }
    let mut child_connections: HashMap<Option<FolderId>, Vec<Connection>> = HashMap::new();
    for connection in connections {
        child_connections
            .entry(connection.folder_id)
            .or_default()
            .push(connection);
    }
    let mut children = Children {
        folders: child_folders,
        connections: child_connections,
        credentials: HashMap::new(),
    };
    for credential in credentials {
        children
            .credentials
            .entry(credential.folder_id)
            .or_default()
            .push(credential);
    }
    ConnectionTree {
        folders: nodes(None, &mut children),
        connections: children.connections.remove(&None).unwrap_or_default(),
        credentials: children.credentials.remove(&None).unwrap_or_default(),
        last_used: None,
    }
}

/// What each folder holds, by folder; the root is `None`.
struct Children {
    folders: HashMap<Option<FolderId>, Vec<ConnectionFolder>>,
    connections: HashMap<Option<FolderId>, Vec<Connection>>,
    credentials: HashMap<Option<FolderId>, Vec<Credential>>,
}

fn nodes(parent: Option<FolderId>, children: &mut Children) -> Vec<FolderNode> {
    let Some(folders) = children.folders.remove(&parent) else {
        return Vec::new();
    };
    folders
        .into_iter()
        .map(|folder| {
            let id = Some(folder.id);
            FolderNode {
                folders: nodes(id, children),
                connections: children.connections.remove(&id).unwrap_or_default(),
                credentials: children.credentials.remove(&id).unwrap_or_default(),
                folder,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "folders_tests.rs"]
mod tests;
