//! The Start menu (`start_menu_items`, migration 0027, RC-100-86): Total Commander's menu
//! "Start", entries, submenus and separators in the order a person gave them.
//!
//! Every write is one `BEGIN IMMEDIATE` transaction that checks before it writes, as the hotlist
//! does: a parent exists and is a submenu, a submenu never moves into itself or below, nothing
//! nests deeper than [`START_MENU_MAX_DEPTH`], the menu holds at most
//! [`MAX_START_MENU_ITEMS`] rows and a field at most [`crate::MAX_USER_COMMAND_FIELD`]
//! characters. A submenu removed takes its rows with it (`ON DELETE CASCADE`).

use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteRow};

use crate::{
    Db,
    user_commands::{
        CommandFields, StoreResult, UserCommandStoreError, check_field, check_fields,
        fields_from_row,
    },
};

/// The most rows of the Start menu, submenus and separators counted.
pub const MAX_START_MENU_ITEMS: usize = 512;

/// How deep the Start menu nests; the top level is 1.
pub const START_MENU_MAX_DEPTH: usize = 8;

/// One row of the Start menu to store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartMenuRecord {
    /// `item`, `submenu` or `separator`.
    pub kind: String,
    pub title: String,
    /// An entry's command; empty fields for a submenu or a separator.
    pub fields: CommandFields,
}

/// One row of the Start menu as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredStartMenuItem {
    pub id: i64,
    /// The submenu it is in; `None` at the top.
    pub parent_id: Option<i64>,
    /// Its place among its siblings.
    pub position: i64,
    pub record: StartMenuRecord,
}

impl StoredStartMenuItem {
    /// Whether it is a submenu, the only kind that holds rows.
    #[must_use]
    pub fn is_submenu(&self) -> bool {
        self.record.kind == "submenu"
    }
}

/// The columns of a row; literal, so every query stays a literal.
macro_rules! columns {
    () => {
        "id, parent_id, position, kind, title, command_kind, command, params, folder, show, \
         elevated"
    };
}

fn item_from_row(row: &SqliteRow) -> sqlx::Result<StoredStartMenuItem> {
    Ok(StoredStartMenuItem {
        id: row.try_get("id")?,
        parent_id: row.try_get("parent_id")?,
        position: row.try_get("position")?,
        record: StartMenuRecord {
            kind: row.try_get("kind")?,
            title: row.try_get("title")?,
            fields: fields_from_row(row, "command_kind")?,
        },
    })
}

/// The walks' limit: a file whose rows were edited into a loop still ends.
fn walk_limit() -> i64 {
    i64::try_from(START_MENU_MAX_DEPTH * 2).unwrap_or(i64::MAX)
}

impl Db {
    /// Every row, the top level first, each level in its order; the engine builds the tree.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn start_menu(&self) -> StoreResult<Vec<StoredStartMenuItem>> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM start_menu_items ORDER BY ifnull(parent_id, 0), position, id"
        ))
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
    /// [`UserCommandStoreError::NotFound`], or the read fails.
    pub async fn start_menu_item(&self, id: i64) -> StoreResult<StoredStartMenuItem> {
        let mut reader = self.readers().acquire().await?;
        fetch(&mut reader, id).await
    }

    /// Adds a row after the last one of `parent` (the top when `None`).
    ///
    /// # Errors
    ///
    /// The menu full, the parent missing or no submenu, nesting too deep, a field too long, or
    /// the write fails; nothing is stored.
    pub async fn add_start_menu_item(
        &self,
        parent: Option<i64>,
        record: &StartMenuRecord,
    ) -> StoreResult<StoredStartMenuItem> {
        check_record(record)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM start_menu_items")
            .fetch_one(&mut *tx)
            .await?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_START_MENU_ITEMS {
            return Err(UserCommandStoreError::TooMany {
                max: MAX_START_MENU_ITEMS,
            });
        }
        if path_to_top(&mut tx, parent).await?.len() + 1 > START_MENU_MAX_DEPTH {
            return Err(UserCommandStoreError::TooDeep {
                max: START_MENU_MAX_DEPTH,
            });
        }
        let position: i64 = sqlx::query_scalar(
            "SELECT ifnull(max(position) + 1, 0) FROM start_menu_items WHERE parent_id IS ?",
        )
        .bind(parent)
        .fetch_one(&mut *tx)
        .await?;
        let id = sqlx::query(
            "INSERT INTO start_menu_items (parent_id, position, kind, title, command_kind, \
             command, params, folder, show, elevated) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(parent)
        .bind(position)
        .bind(&record.kind)
        .bind(&record.title)
        .bind(&record.fields.kind)
        .bind(&record.fields.command)
        .bind(&record.fields.params)
        .bind(&record.fields.folder)
        .bind(&record.fields.show)
        .bind(record.fields.elevated)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
        let item = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(item)
    }

    /// Replaces the kind, title and command of the row `id`; its place stays.
    ///
    /// # Errors
    ///
    /// The row missing, a submenu with rows made another kind, a field too long, or the write
    /// fails; nothing changes.
    pub async fn update_start_menu_item(
        &self,
        id: i64,
        record: &StartMenuRecord,
    ) -> StoreResult<StoredStartMenuItem> {
        check_record(record)?;
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let current = fetch(&mut tx, id).await?;
        if current.is_submenu() && record.kind != "submenu" {
            let children: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM start_menu_items WHERE parent_id = ?")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            if children > 0 {
                return Err(UserCommandStoreError::NotEmpty { id });
            }
        }
        sqlx::query(
            "UPDATE start_menu_items SET kind = ?, title = ?, command_kind = ?, command = ?, \
             params = ?, folder = ?, show = ?, elevated = ?, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(&record.kind)
        .bind(&record.title)
        .bind(&record.fields.kind)
        .bind(&record.fields.command)
        .bind(&record.fields.params)
        .bind(&record.fields.folder)
        .bind(&record.fields.show)
        .bind(record.fields.elevated)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let item = fetch(&mut tx, id).await?;
        tx.commit().await?;
        Ok(item)
    }

    /// Moves a row, a submenu with everything in it, into `parent` (the top when `None`) at
    /// `position` among its rows, or after the last; the rows there are numbered anew.
    ///
    /// # Errors
    ///
    /// A row missing, the parent no submenu, a submenu into itself or below, nesting too deep,
    /// or the write fails; the order stays as it was.
    pub async fn move_start_menu_item(
        &self,
        id: i64,
        parent: Option<i64>,
        position: Option<usize>,
    ) -> StoreResult<StoredStartMenuItem> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        fetch(&mut tx, id).await?;
        let above = path_to_top(&mut tx, parent).await?;
        if above.contains(&id) {
            return Err(UserCommandStoreError::Cycle);
        }
        if above.len() + subtree_height(&mut tx, id).await? > START_MENU_MAX_DEPTH {
            return Err(UserCommandStoreError::TooDeep {
                max: START_MENU_MAX_DEPTH,
            });
        }
        let mut siblings: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM start_menu_items WHERE parent_id IS ? AND id <> ? \
             ORDER BY position, id",
        )
        .bind(parent)
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let at = position.unwrap_or(siblings.len()).min(siblings.len());
        siblings.insert(at, id);
        for (order, sibling) in siblings.iter().enumerate() {
            sqlx::query(
                "UPDATE start_menu_items SET parent_id = ?, position = ?, \
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

    /// Removes a row; a submenu goes with everything in it. The rows removed, the row itself
    /// counted.
    ///
    /// # Errors
    ///
    /// The row missing, or the write fails.
    pub async fn delete_start_menu_item(&self, id: i64) -> StoreResult<u64> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        fetch(&mut tx, id).await?;
        let removed: i64 = sqlx::query_scalar(
            "WITH RECURSIVE below(id, level) AS (SELECT ?, 1 UNION ALL \
             SELECT start_menu_items.id, below.level + 1 FROM start_menu_items JOIN below \
             ON start_menu_items.parent_id = below.id WHERE below.level < ?) \
             SELECT COUNT(*) FROM below",
        )
        .bind(id)
        .bind(walk_limit())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM start_menu_items WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(u64::try_from(removed).unwrap_or(0))
    }
}

/// Refuses a row whose fields are too long.
fn check_record(record: &StartMenuRecord) -> StoreResult<()> {
    check_field("title", &record.title)?;
    check_fields(&record.fields)
}

async fn fetch(connection: &mut SqliteConnection, id: i64) -> StoreResult<StoredStartMenuItem> {
    let row = sqlx::query(concat!(
        "SELECT ",
        columns!(),
        " FROM start_menu_items WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or(UserCommandStoreError::NotFound { id })?;
    Ok(item_from_row(&row)?)
}

/// The submenu `parent` and the submenus above it, nearest first; empty for the top. The parent
/// must be a submenu.
async fn path_to_top(
    connection: &mut SqliteConnection,
    parent: Option<i64>,
) -> StoreResult<Vec<i64>> {
    let Some(parent) = parent else {
        return Ok(Vec::new());
    };
    if !fetch(connection, parent).await?.is_submenu() {
        return Err(UserCommandStoreError::NotASubmenu { id: parent });
    }
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE up(id, parent_id) AS (SELECT id, parent_id FROM start_menu_items \
         WHERE id = ? UNION ALL SELECT start_menu_items.id, start_menu_items.parent_id \
         FROM start_menu_items JOIN up ON start_menu_items.id = up.parent_id) \
         SELECT id FROM up LIMIT ?",
    )
    .bind(parent)
    .bind(walk_limit())
    .fetch_all(&mut *connection)
    .await?)
}

/// The levels of `id`'s subtree, itself counted: 1 for an entry or an empty submenu.
async fn subtree_height(connection: &mut SqliteConnection, id: i64) -> StoreResult<usize> {
    let height: i64 = sqlx::query_scalar(
        "WITH RECURSIVE below(id, level) AS (SELECT ?, 1 UNION ALL \
         SELECT start_menu_items.id, below.level + 1 FROM start_menu_items JOIN below \
         ON start_menu_items.parent_id = below.id WHERE below.level < ?) \
         SELECT max(level) FROM below",
    )
    .bind(id)
    .bind(walk_limit())
    .fetch_one(&mut *connection)
    .await?;
    Ok(usize::try_from(height).unwrap_or(START_MENU_MAX_DEPTH + 1))
}

#[cfg(test)]
#[path = "start_menu_tests.rs"]
mod tests;
