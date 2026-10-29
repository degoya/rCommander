//! The multi-rename undo log (RC-020-01, migration 0011): every step of a run, written before
//! the run's first rename, and a mark per step once it is carried out.
//!
//! The rows of a run are its steps in order (`seq` from 0); an undo reverses the steps marked
//! `done`, last first, and clears each mark once that step is reversed - so the marks are always
//! a prefix of the run, and after a crash the step at the boundary is the only one in doubt (the
//! engine reads it off the directory, `rc_rename::applied_count`). The table is retained
//! ([`crate::RETAINED_TABLES`]): the sweep deletes the oldest rows first and may cut the oldest
//! run, which [`RenameRun::is_complete`] then tells.

use serde::{Deserialize, Serialize};
use sqlx::Connection as _;

use crate::{Db, Result};

/// One step of a logged run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggedStep {
    /// The name before the step.
    pub from: String,
    /// The name after it.
    pub to: String,
    /// Carried out (and not reversed since).
    pub done: bool,
}

/// A run of the undo log, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameRun {
    /// The run's tag, the one in its temporary names.
    pub run: String,
    /// The directory the run renames in, as the engine stored it.
    pub dir: String,
    /// When the run was logged, `strftime('%Y-%m-%dT%H:%M:%fZ')`.
    pub recorded_at: String,
    /// The run's length as it was written.
    pub total: u64,
    /// The steps still stored, in order.
    pub steps: Vec<LoggedStep>,
    /// Whether the stored steps are the run's `0..total`, none cut by the sweep.
    complete: bool,
}

impl RenameRun {
    /// Whether every step of the run is still stored; an undo needs all of them.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// How many steps from the first one on are marked done.
    #[must_use]
    pub fn marked(&self) -> usize {
        self.steps.iter().take_while(|step| step.done).count()
    }
}

/// `seq`, `steps`, `from_name`, `to_name`, `done`, `recorded_at`, `dir`.
type Row = (i64, i64, String, String, i64, String, String);

fn int(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl Db {
    /// Writes every step of the run `run` in `dir`, in order and not done, in one transaction.
    /// Nothing is written for a run without steps.
    ///
    /// # Errors
    ///
    /// The write fails, or `run` is logged already; then nothing is written.
    pub async fn log_rename_run(&self, run: &str, dir: &str, steps: &[(&str, &str)]) -> Result<()> {
        if steps.is_empty() {
            return Ok(());
        }
        let total = int(steps.len());
        let mut writer = self.writer().await;
        let mut transaction = writer.begin_with("BEGIN IMMEDIATE").await?;
        for (seq, (from, to)) in steps.iter().enumerate() {
            sqlx::query(
                "INSERT INTO rename_log (run, dir, seq, steps, from_name, to_name) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(run)
            .bind(dir)
            .bind(int(seq))
            .bind(total)
            .bind(*from)
            .bind(*to)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Marks step `seq` of `run` done or not done.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn mark_rename_step(&self, run: &str, seq: usize, done: bool) -> Result<()> {
        let mut writer = self.writer().await;
        sqlx::query("UPDATE rename_log SET done = ? WHERE run = ? AND seq = ?")
            .bind(i64::from(done))
            .bind(run)
            .bind(int(seq))
            .execute(&mut *writer)
            .await?;
        Ok(())
    }

    /// Marks the first `applied` steps of `run` done and the rest not done: the marks after the
    /// directory was read following a crash.
    ///
    /// # Errors
    ///
    /// The write fails.
    pub async fn set_rename_marks(&self, run: &str, applied: usize) -> Result<()> {
        let mut writer = self.writer().await;
        sqlx::query("UPDATE rename_log SET done = (seq < ?) WHERE run = ?")
            .bind(int(applied))
            .bind(run)
            .execute(&mut *writer)
            .await?;
        Ok(())
    }

    /// The latest run of the log, the one an undo reverses; `None` when the log is empty.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn latest_rename_run(&self) -> Result<Option<RenameRun>> {
        let run: Option<String> =
            sqlx::query_scalar("SELECT run FROM rename_log ORDER BY id DESC LIMIT 1")
                .fetch_optional(self.readers())
                .await?;
        let Some(run) = run else {
            return Ok(None);
        };
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT seq, steps, from_name, to_name, done, recorded_at, dir FROM rename_log \
             WHERE run = ? ORDER BY seq",
        )
        .bind(&run)
        .fetch_all(self.readers())
        .await?;
        let Some((_, total, _, _, _, recorded_at, dir)) = rows.first().cloned() else {
            // Swept between the two reads.
            return Ok(None);
        };
        let complete = rows.len() == usize::try_from(total).unwrap_or(0)
            && rows
                .iter()
                .enumerate()
                .all(|(index, row)| row.0 == int(index) && row.1 == total);
        Ok(Some(RenameRun {
            run,
            dir,
            recorded_at,
            total: u64::try_from(total).unwrap_or(0),
            steps: rows
                .into_iter()
                .map(|(_, _, from, to, done, _, _)| LoggedStep {
                    from,
                    to,
                    done: done != 0,
                })
                .collect(),
            complete,
        }))
    }
}

#[cfg(test)]
#[path = "rename_log_tests.rs"]
mod tests;
