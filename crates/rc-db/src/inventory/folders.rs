//! The folders a sync puts its connections into (RC-100-39): the source's own, made at the root
//! under the source's name when it has none, and the folder path of each host below it.

use sqlx::SqliteConnection;

use super::{InventoryResult, InventorySource};
use crate::connections::FolderId;

/// The source's folder: the stored one, else (when writing) the root's folder named after the
/// source, made when there is none.
pub(super) async fn target_folder(
    connection: &mut SqliteConnection,
    source: &InventorySource,
    write: bool,
) -> InventoryResult<Option<FolderId>> {
    if let Some(folder) = source.folder_id {
        return Ok(Some(folder));
    }
    let found = find_folder(connection, None, &source.name).await?;
    match (found, write) {
        (Some(folder), _) => Ok(Some(folder)),
        (None, true) => Ok(Some(insert_folder(connection, None, &source.name).await?)),
        (None, false) => Ok(None),
    }
}

/// The folder `path` below `top`, its missing folders made when writing; `None` in a preview
/// when one is still missing.
pub(super) async fn subfolder(
    connection: &mut SqliteConnection,
    top: FolderId,
    path: &[String],
    write: bool,
) -> InventoryResult<Option<FolderId>> {
    let mut folder = top;
    for name in path {
        folder = match find_folder(connection, Some(folder), name).await? {
            Some(found) => found,
            None if write => insert_folder(connection, Some(folder), name).await?,
            None => return Ok(None),
        };
    }
    Ok(Some(folder))
}

/// The folder `name` (ignoring ASCII case) in `parent`, the root for `None`.
async fn find_folder(
    connection: &mut SqliteConnection,
    parent: Option<FolderId>,
    name: &str,
) -> InventoryResult<Option<FolderId>> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM connection_folders WHERE parent_id IS ? AND name = ? COLLATE NOCASE",
    )
    .bind(parent.map(|parent| parent.0))
    .bind(name)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(id.map(FolderId))
}

/// A new folder `name` after `parent`'s last folder.
async fn insert_folder(
    connection: &mut SqliteConnection,
    parent: Option<FolderId>,
    name: &str,
) -> InventoryResult<FolderId> {
    let parent = parent.map(|parent| parent.0);
    let done = sqlx::query(
        "INSERT INTO connection_folders (parent_id, name, sort_order) VALUES (?, ?, \
         (SELECT ifnull(max(sort_order) + 1, 0) FROM connection_folders WHERE parent_id IS ?))",
    )
    .bind(parent)
    .bind(name)
    .bind(parent)
    .execute(&mut *connection)
    .await?;
    Ok(FolderId(done.last_insert_rowid()))
}
