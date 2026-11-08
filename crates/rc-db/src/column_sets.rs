//! Column sets (RC-100-90, migration 0029): the person's own columns of the file list, saved
//! under a name that is unique without regard to case, in the order the person put them.
//!
//! The spec is a JSON object the engine wrote and checked (`rc_engine::columns::ColumnSetSpec`);
//! this module keeps the text and nothing more, and the engine checks what it reads back like any
//! other input. The table is durable ([`crate::DURABLE_TABLES`]): a person makes and removes
//! column sets.

use sqlx::{Connection as _, SqliteConnection};

use crate::{Db, Result};

/// One column set as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredColumnSet {
    pub id: i64,
    pub name: String,
    /// Its place in the list, smallest first.
    pub position: i64,
    /// The engine's JSON.
    pub spec: String,
}

/// `id`, `name`, `position`, `spec`.
type Row = (i64, String, i64, String);

/// The columns of a column set; literal, so every query stays a literal.
macro_rules! columns {
    () => {
        "id, name, position, spec"
    };
}

fn from_row((id, name, position, spec): Row) -> StoredColumnSet {
    StoredColumnSet {
        id,
        name,
        position,
        spec,
    }
}

impl Db {
    /// Every column set in the person's order.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn column_sets(&self) -> Result<Vec<StoredColumnSet>> {
        let rows: Vec<Row> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM column_sets ORDER BY position, id"
        ))
        .fetch_all(self.readers())
        .await?;
        Ok(rows.into_iter().map(from_row).collect())
    }

    /// The column set `id`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn column_set(&self, id: i64) -> Result<Option<StoredColumnSet>> {
        let row: Option<Row> = sqlx::query_as(concat!(
            "SELECT ",
            columns!(),
            " FROM column_sets WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(self.readers())
        .await?;
        Ok(row.map(from_row))
    }

    /// Stores a new column set after the last one; the set as stored. The caller checks the
    /// name and the count: a name another set has (case does not count) fails the unique
    /// constraint.
    ///
    /// # Errors
    ///
    /// A blank, too long or taken name, a spec that is not JSON or too long, or the write fails;
    /// nothing is stored.
    pub async fn create_column_set(&self, name: &str, spec: &str) -> Result<StoredColumnSet> {
        let mut writer = self.writer().await;
        let row: Row = sqlx::query_as(concat!(
            "INSERT INTO column_sets (name, position, spec) VALUES \
             (?, (SELECT ifnull(max(position) + 1, 0) FROM column_sets), ?) RETURNING ",
            columns!()
        ))
        .bind(name)
        .bind(spec)
        .fetch_one(&mut *writer)
        .await?;
        Ok(from_row(row))
    }

    /// Changes the name and/or the spec of the column set `id` (`None` leaves it); the set as
    /// stored now, `None` when there is none.
    ///
    /// # Errors
    ///
    /// A blank, too long or taken name, a spec that is not JSON or too long, or the write fails;
    /// nothing changes.
    pub async fn update_column_set(
        &self,
        id: i64,
        name: Option<&str>,
        spec: Option<&str>,
    ) -> Result<Option<StoredColumnSet>> {
        let mut writer = self.writer().await;
        let row: Option<Row> = sqlx::query_as(concat!(
            "UPDATE column_sets SET name = coalesce(?, name), spec = coalesce(?, spec), \
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? RETURNING ",
            columns!()
        ))
        .bind(name)
        .bind(spec)
        .bind(id)
        .fetch_optional(&mut *writer)
        .await?;
        Ok(row.map(from_row))
    }

    /// Puts the column set `id` at `position` (from 0, the last place when beyond it) and
    /// numbers the list anew, in one transaction; whether there was such a set.
    ///
    /// # Errors
    ///
    /// The write fails; the order stays as it was.
    pub async fn move_column_set(&self, id: i64, position: usize) -> Result<bool> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let mut order: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM column_sets ORDER BY position, id")
                .fetch_all(&mut *tx)
                .await?;
        let Some(from) = order.iter().position(|&known| known == id) else {
            return Ok(false);
        };
        let moved = order.remove(from);
        order.insert(position.min(order.len()), moved);
        renumber(&mut tx, &order).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Removes the column set `id`; whether there was one.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn delete_column_set(&self, id: i64) -> Result<bool> {
        let mut writer = self.writer().await;
        let deleted = sqlx::query("DELETE FROM column_sets WHERE id = ?")
            .bind(id)
            .execute(&mut *writer)
            .await?
            .rows_affected();
        Ok(deleted > 0)
    }
}

/// Gives the column sets of `order` the places 0, 1, 2, ... in that order.
async fn renumber(connection: &mut SqliteConnection, order: &[i64]) -> Result<()> {
    for (place, id) in order.iter().enumerate() {
        sqlx::query("UPDATE column_sets SET position = ? WHERE id = ?")
            .bind(i64::try_from(place).unwrap_or(i64::MAX))
            .bind(*id)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "column_sets_tests.rs"]
mod tests;
