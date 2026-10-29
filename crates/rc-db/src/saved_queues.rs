//! Saved transfer lists (RC-010-15, migration 0015): a list of the operations queue a person saved
//! under a name, with its jobs in their order, to load it again later.
//!
//! The jobs are text the engine wrote (`rc_ops::JobSpec` as JSON: paths and connection ids, never
//! a secret); this module keeps them and nothing more, and the engine checks what it reads back
//! like any other input. Saving under a name that exists replaces that list in one transaction.
//! The tables are durable ([`crate::DURABLE_TABLES`]): a person makes and removes the lists.

use sqlx::Connection as _;

use crate::{Db, Result};

/// One saved list, without its jobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedQueue {
    pub id: i64,
    pub name: String,
    /// Loaded again, stopped, when the app starts.
    pub restore_on_start: bool,
    /// When it was saved last, `strftime('%Y-%m-%dT%H:%M:%fZ')`.
    pub saved_at: String,
    /// How many jobs it holds.
    pub entries: u64,
}

/// `id`, `name`, `restore_on_start`, `saved_at`, entry count.
type Row = (i64, String, i64, String, i64);

/// The columns of a saved list with its entry count; literal, so every query stays a literal.
macro_rules! select {
    () => {
        "SELECT q.id, q.name, q.restore_on_start, q.saved_at, \
         (SELECT count(*) FROM saved_queue_entries e WHERE e.queue_id = q.id) \
         FROM saved_queues q"
    };
}

fn from_row((id, name, restore_on_start, saved_at, entries): Row) -> SavedQueue {
    SavedQueue {
        id,
        name,
        restore_on_start: restore_on_start != 0,
        saved_at,
        entries: u64::try_from(entries).unwrap_or_default(),
    }
}

impl Db {
    /// Saves the jobs `specs` (each a JSON text) in their order as the list `name`, replacing a
    /// saved list of that name (case does not count); the list as saved.
    ///
    /// # Errors
    ///
    /// A blank name, a job that is not JSON, or the write fails; nothing changes then.
    pub async fn save_queue(
        &self,
        name: &str,
        restore_on_start: bool,
        specs: &[String],
    ) -> Result<SavedQueue> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO saved_queues (name, restore_on_start) VALUES (?, ?) \
             ON CONFLICT (name) DO UPDATE SET name = excluded.name, \
             restore_on_start = excluded.restore_on_start, \
             saved_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
             RETURNING id",
        )
        .bind(name)
        .bind(i64::from(restore_on_start))
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM saved_queue_entries WHERE queue_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for (position, spec) in specs.iter().enumerate() {
            sqlx::query(
                "INSERT INTO saved_queue_entries (queue_id, position, spec) VALUES (?, ?, ?)",
            )
            .bind(id)
            .bind(i64::try_from(position).unwrap_or(i64::MAX))
            .bind(spec)
            .execute(&mut *tx)
            .await?;
        }
        let row: Row = sqlx::query_as(concat!(select!(), " WHERE q.id = ?"))
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(from_row(row))
    }

    /// Every saved list, by name.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn saved_queues(&self) -> Result<Vec<SavedQueue>> {
        let rows: Vec<Row> = sqlx::query_as(concat!(select!(), " ORDER BY q.name COLLATE NOCASE"))
            .fetch_all(self.readers())
            .await?;
        Ok(rows.into_iter().map(from_row).collect())
    }

    /// The saved list `id` and its jobs (JSON texts) in their order; `None` when there is none.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn saved_queue(&self, id: i64) -> Result<Option<(SavedQueue, Vec<String>)>> {
        let row: Option<Row> = sqlx::query_as(concat!(select!(), " WHERE q.id = ?"))
            .bind(id)
            .fetch_optional(self.readers())
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let specs: Vec<String> = sqlx::query_scalar(
            "SELECT spec FROM saved_queue_entries WHERE queue_id = ? ORDER BY position",
        )
        .bind(id)
        .fetch_all(self.readers())
        .await?;
        Ok(Some((from_row(row), specs)))
    }

    /// Removes the saved list `id` with its jobs; whether there was one.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn delete_saved_queue(&self, id: i64) -> Result<bool> {
        let mut writer = self.writer().await;
        let deleted = sqlx::query("DELETE FROM saved_queues WHERE id = ?")
            .bind(id)
            .execute(&mut *writer)
            .await?
            .rows_affected();
        Ok(deleted > 0)
    }
}

#[cfg(test)]
#[path = "saved_queues_tests.rs"]
mod tests;
