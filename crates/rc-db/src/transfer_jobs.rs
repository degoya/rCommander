//! The transfer jobs that have not ended (RC-060-07, migration 0014): a row per copy or move to or
//! from a server while it runs, so one the app left in the middle is put back into the queue at
//! the next start.
//!
//! The rows are text the engine wrote: the job as JSON and the file it was writing as JSON. This
//! module keeps them and nothing more; reading them back is the engine's, which checks what it
//! reads like any other input. The table is retained ([`crate::RETAINED_TABLES`]): a job nobody
//! resumed or discarded goes with the sweep.

use serde::{Deserialize, Serialize};

use crate::{Db, Result};

/// One kept job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredTransfer {
    pub id: i64,
    /// When the job started, `strftime('%Y-%m-%dT%H:%M:%fZ')`.
    pub recorded_at: String,
    /// The job as submitted (`rc_ops::JobSpec` as JSON).
    pub spec: String,
    /// The file it was writing (`rc_ops::FileMark` as JSON), when it marked one.
    pub file: Option<String>,
}

impl Db {
    /// Keeps a transfer job that starts; the id of its row.
    ///
    /// # Errors
    ///
    /// The write fails, or `spec` is not JSON.
    pub async fn save_transfer(&self, spec: &str) -> Result<i64> {
        let mut writer = self.writer().await;
        let id = sqlx::query_scalar("INSERT INTO transfer_jobs (spec) VALUES (?) RETURNING id")
            .bind(spec)
            .fetch_one(&mut *writer)
            .await?;
        Ok(id)
    }

    /// The file the job of row `id` writes now; nothing when the row is gone.
    ///
    /// # Errors
    ///
    /// The write fails, or `file` is not JSON.
    pub async fn mark_transfer(&self, id: i64, file: &str) -> Result<()> {
        let mut writer = self.writer().await;
        sqlx::query("UPDATE transfer_jobs SET file = ? WHERE id = ?")
            .bind(file)
            .bind(id)
            .execute(&mut *writer)
            .await?;
        Ok(())
    }

    /// Forgets the job of row `id`: it completed, was cancelled or failed.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn forget_transfer(&self, id: i64) -> Result<()> {
        let mut writer = self.writer().await;
        sqlx::query("DELETE FROM transfer_jobs WHERE id = ?")
            .bind(id)
            .execute(&mut *writer)
            .await?;
        Ok(())
    }

    /// Every kept job, oldest first.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn transfers(&self) -> Result<Vec<StoredTransfer>> {
        let rows: Vec<(i64, String, String, Option<String>)> =
            sqlx::query_as("SELECT id, recorded_at, spec, file FROM transfer_jobs ORDER BY id")
                .fetch_all(self.readers())
                .await?;
        Ok(rows
            .into_iter()
            .map(|(id, recorded_at, spec, file)| StoredTransfer {
                id,
                recorded_at,
                spec,
                file,
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "transfer_jobs_tests.rs"]
mod tests;
