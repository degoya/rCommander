//! The directory hotlist (`hotlist`, migration 0008): Total Commander's Ctrl+D menu, entries
//! and folders (submenus) in the order a person gave them.
//!
//! A row without a location is a folder; only a folder holds rows. Every write is one `BEGIN
//! IMMEDIATE` transaction that checks before it writes: a parent exists and is a folder, a folder
//! never moves into itself or below, and nothing nests deeper than [`HOTLIST_MAX_DEPTH`].
//! Locations are the engine's `VfsPath` JSON, opaque here.

use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteRow};

use crate::{Db, DbError};

/// How deep the hotlist nests; the top level is 1.
pub const HOTLIST_MAX_DEPTH: usize = 8;

/// The longest name, in characters.
pub const HOTLIST_MAX_NAME: usize = 200;

/// One entry or folder of the hotlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotlistItem {
    pub id: i64,
    /// The folder it is in; `None` at the top.
    pub parent_id: Option<i64>,
    pub name: String,
    /// The folder it opens, as `VfsPath` JSON; `None` for a hotlist folder.
    pub location: Option<String>,
    /// What the other panel shows with it, as `VfsPath` JSON.
    pub target: Option<String>,
    /// Its place among its siblings.
    pub sort_order: i64,
}

impl HotlistItem {
    /// Whether it is a folder of the hotlist (a submenu), not an entry.
    #[must_use]
    pub fn is_folder(&self) -> bool {
        self.location.is_none()
    }
}

/// A row to add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewHotlistItem<'a> {
    pub parent_id: Option<i64>,
    pub name: &'a str,
    /// `None` adds a folder.
    pub location: Option<&'a str>,
    /// Only for an entry; a folder's is dropped.
    pub target: Option<&'a str>,
}

/// What an update changes; `None` keeps a field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HotlistPatch {
    pub name: Option<String>,
    pub location: Option<String>,
    /// `Some(None)` removes the target.
    pub target: Option<Option<String>>,
}

/// Why a hotlist change was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum HotlistError {
    #[error("there is no hotlist entry {id}")]
    NotFound { id: i64 },
    #[error("hotlist entry {id} is not a folder")]
    NotAFolder { id: i64 },
    #[error("hotlist entry {id} is a folder; it has no path")]
    NotAnEntry { id: i64 },
    #[error("a hotlist folder cannot move into itself or one of its folders")]
    Cycle,
    #[error("the hotlist nests at most {max} levels deep")]
    TooDeep { max: usize },
    #[error("a hotlist name is not blank and has at most {max} characters")]
    BadName { max: usize },
    #[error(transparent)]
    Db(#[from] DbError),
}

impl From<sqlx::Error> for HotlistError {
    fn from(error: sqlx::Error) -> Self {
        Self::Db(DbError::Sqlx(error))
    }
}

type HotlistResult<T> = Result<T, HotlistError>;

fn item_from_row(row: &SqliteRow) -> sqlx::Result<HotlistItem> {
    Ok(HotlistItem {
        id: row.try_get("id")?,
        parent_id: row.try_get("parent_id")?,
        name: row.try_get("name")?,
        location: row.try_get("location")?,
        target: row.try_get("target")?,
        sort_order: row.try_get("sort_order")?,
    })
}

/// `name` trimmed, when it is not blank and not too long.
fn checked_name(name: &str) -> HotlistResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > HOTLIST_MAX_NAME {
        return Err(HotlistError::BadName {
            max: HOTLIST_MAX_NAME,
        });
    }
    Ok(name.to_owned())
}

impl Db {
    /// Every row, top level first, each level in its order.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn hotlist(&self) -> HotlistResult<Vec<HotlistItem>> {
        let rows = sqlx::query(
            "SELECT id, parent_id, name, location, target, sort_order FROM hotlist \
             ORDER BY ifnull(parent_id, 0), sort_order, id",
        )
        .fetch_all(self.readers())
        .await?;
        Ok(rows
            .iter()
            .map(item_from_row)
            .collect::<sqlx::Result<Vec<_>>>()?)
    }

    /// The row `id`.
    ///
    /// # Errors
    ///
    /// [`HotlistError::NotFound`], or the read fails.
    pub async fn hotlist_item(&self, id: i64) -> HotlistResult<HotlistItem> {
        let mut reader = self.readers().acquire().await?;
        fetch(&mut reader, id).await
    }

    /// Adds an entry or a folder after the last row of its folder.
    ///
    /// # Errors
    ///
    /// A blank name, the parent missing or no folder, nesting too deep, or the write fails.
    pub async fn add_hotlist(&self, new: &NewHotlistItem<'_>) -> HotlistResult<HotlistItem> {
        let name = checked_name(new.name)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let depth = depth_below(&mut tx, new.parent_id).await?;
        if depth + 1 > HOTLIST_MAX_DEPTH {
            return Err(HotlistError::TooDeep {
                max: HOTLIST_MAX_DEPTH,
            });
        }
        let sort_order: i64 = sqlx::query_scalar(
            "SELECT ifnull(max(sort_order) + 1, 0) FROM hotlist WHERE parent_id IS ?",
        )
        .bind(new.parent_id)
        .fetch_one(&mut *tx)
        .await?;
        let id = sqlx::query(
            "INSERT INTO hotlist (parent_id, name, location, target, sort_order) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(new.parent_id)
        .bind(&name)
        .bind(new.location)
        // A folder has no target.
        .bind(new.location.and(new.target))
        .bind(sort_order)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
        let item = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(item)
    }

    /// Changes a row's name, location or target. A folder takes neither a location nor a
    /// target.
    ///
    /// # Errors
    ///
    /// The row missing, a blank name, a location or target for a folder, or the write fails.
    pub async fn update_hotlist(
        &self,
        id: i64,
        patch: &HotlistPatch,
    ) -> HotlistResult<HotlistItem> {
        let name = patch.name.as_deref().map(checked_name).transpose()?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let current = fetch(&mut tx, id).await?;
        if current.is_folder() && (patch.location.is_some() || patch.target.is_some()) {
            return Err(HotlistError::NotAnEntry { id });
        }
        let target = match &patch.target {
            Some(target) => target.clone(),
            None => current.target.clone(),
        };
        sqlx::query(
            "UPDATE hotlist SET name = ?, location = ?, target = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(name.unwrap_or(current.name))
        .bind(patch.location.clone().or(current.location))
        .bind(target)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let item = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(item)
    }

    /// Moves a row, a folder with everything in it, into `parent` (the top when `None`) at
    /// `position` among its rows, or after the last; the rows there are numbered anew.
    ///
    /// # Errors
    ///
    /// A row missing, the parent no folder, a folder into itself or below, nesting too deep, or
    /// the write fails.
    pub async fn move_hotlist(
        &self,
        id: i64,
        parent: Option<i64>,
        position: Option<usize>,
    ) -> HotlistResult<HotlistItem> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        fetch(&mut tx, id).await?;
        let above = path_to_top(&mut tx, parent).await?;
        if above.contains(&id) {
            return Err(HotlistError::Cycle);
        }
        if above.len() + subtree_height(&mut tx, id).await? > HOTLIST_MAX_DEPTH {
            return Err(HotlistError::TooDeep {
                max: HOTLIST_MAX_DEPTH,
            });
        }
        let mut siblings: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM hotlist WHERE parent_id IS ? AND id <> ? ORDER BY sort_order, id",
        )
        .bind(parent)
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let at = position.unwrap_or(siblings.len()).min(siblings.len());
        siblings.insert(at, id);
        for (order, sibling) in siblings.iter().enumerate() {
            sqlx::query(
                "UPDATE hotlist SET parent_id = ?, sort_order = ?, \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
            )
            .bind(parent)
            .bind(i64::try_from(order).unwrap_or(i64::MAX))
            .bind(*sibling)
            .execute(&mut *tx)
            .await?;
        }
        let item = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(item)
    }

    /// Removes a row; a folder goes with everything in it. The rows removed, the row itself
    /// counted.
    ///
    /// # Errors
    ///
    /// The row missing, or the write fails.
    pub async fn delete_hotlist(&self, id: i64) -> HotlistResult<u64> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        fetch(&mut tx, id).await?;
        let limit = i64::try_from(HOTLIST_MAX_DEPTH * 2).unwrap_or(i64::MAX);
        let removed: i64 = sqlx::query_scalar(
            "WITH RECURSIVE below(id, level) AS (SELECT ?, 1 UNION ALL \
             SELECT hotlist.id, below.level + 1 FROM hotlist JOIN below \
             ON hotlist.parent_id = below.id WHERE below.level < ?) SELECT COUNT(*) FROM below",
        )
        .bind(id)
        .bind(limit)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM hotlist WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(u64::try_from(removed).unwrap_or(0))
    }
}

async fn fetch(connection: &mut SqliteConnection, id: i64) -> HotlistResult<HotlistItem> {
    let row = sqlx::query(
        "SELECT id, parent_id, name, location, target, sort_order FROM hotlist WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or(HotlistError::NotFound { id })?;
    Ok(item_from_row(&row)?)
}

/// The folder `parent` and the folders above it, nearest first; empty for the top. The parent
/// must be a folder.
async fn path_to_top(
    connection: &mut SqliteConnection,
    parent: Option<i64>,
) -> HotlistResult<Vec<i64>> {
    let Some(parent) = parent else {
        return Ok(Vec::new());
    };
    if !fetch(connection, parent).await?.is_folder() {
        return Err(HotlistError::NotAFolder { id: parent });
    }
    // The limit stops the walk even in a file whose rows were edited into a loop.
    let limit = i64::try_from(HOTLIST_MAX_DEPTH * 2).unwrap_or(i64::MAX);
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE up(id, parent_id) AS (SELECT id, parent_id FROM hotlist WHERE id = ? \
         UNION ALL SELECT hotlist.id, hotlist.parent_id FROM hotlist JOIN up \
         ON hotlist.id = up.parent_id) SELECT id FROM up LIMIT ?",
    )
    .bind(parent)
    .bind(limit)
    .fetch_all(&mut *connection)
    .await?)
}

/// How many levels lie above a row added to `parent`.
async fn depth_below(
    connection: &mut SqliteConnection,
    parent: Option<i64>,
) -> HotlistResult<usize> {
    Ok(path_to_top(connection, parent).await?.len())
}

/// The levels of `id`'s subtree, itself counted: 1 for an entry or an empty folder.
async fn subtree_height(connection: &mut SqliteConnection, id: i64) -> HotlistResult<usize> {
    let limit = i64::try_from(HOTLIST_MAX_DEPTH * 2).unwrap_or(i64::MAX);
    let height: i64 = sqlx::query_scalar(
        "WITH RECURSIVE below(id, level) AS (SELECT ?, 1 UNION ALL \
         SELECT hotlist.id, below.level + 1 FROM hotlist JOIN below \
         ON hotlist.parent_id = below.id WHERE below.level < ?) SELECT max(level) FROM below",
    )
    .bind(id)
    .bind(limit)
    .fetch_one(&mut *connection)
    .await?;
    Ok(usize::try_from(height).unwrap_or(HOTLIST_MAX_DEPTH + 1))
}

#[cfg(test)]
#[path = "hotlist_tests.rs"]
mod tests;
