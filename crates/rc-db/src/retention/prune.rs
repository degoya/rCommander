//! The sweep that keeps the retained tables within their limits and gives the space back,
//! after rDownloader's `rd-db/src/retention.rs` and `rd-diagnostics/src/sink.rs`.
//!
//! Deletes run in batches of [`PRUNE_BATCH`] rows, each on the writer and released before the
//! next, with a yield between them: a sweep of a neglected table never holds the one writer
//! long enough to stall a setting the person just changed. Age first, then count: a row older
//! than the age limit goes whatever the count, then the oldest go while more than the row limit
//! remain.
//!
//! The file is opened with `auto_vacuum = INCREMENTAL` (`Db::connect`), so freed pages can be
//! returned: whenever many are free - after this sweep's deletes or left by one that was cut
//! short - `PRAGMA incremental_vacuum` hands them back in chunks of 2 048 pages, the
//! writer released between two, and a `wal_checkpoint(TRUNCATE)` empties the write-ahead log the
//! deletes filled.

use sqlx::{AssertSqlSafe, SqliteConnection};

use super::{RetainedTable, RetentionLimits, RetentionSettings};
use crate::{Db, Result};

/// Rows one delete removes at most (rDownloader's `PRUNE_BATCH`).
pub const PRUNE_BATCH: u64 = 2_000;

/// Free pages from which a sweep returns them to the file system: 1 MiB at SQLite's default
/// 4 KiB page. Fewer are left for the next inserts to reuse.
const VACUUM_FREE_PAGES: i64 = 256;

/// One chunk of the vacuum: at most 2 048 free pages, 8 MiB at a 4 KiB page. A file a flood had
/// grown to 13 GB held 3 million free pages; one statement for all of them held the writer for
/// half a minute, chunks of 25 600 still for up to six seconds. The argument cannot be bound.
const VACUUM_CHUNK_SQL: &str = "PRAGMA incremental_vacuum(2048)";

/// What one [`Db::prune`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Rows deleted, over every table.
    pub deleted: u64,
    /// Pages returned to the file system.
    pub freed_pages: u64,
}

/// What one batch did.
struct Step {
    deleted: u64,
    /// Whether rows over a limit are left for another batch.
    more: bool,
}

impl Db {
    /// Deletes what the limits no longer keep, in every retained table, then gives the space
    /// back. Runs at start and every [`crate::SWEEP_INTERVAL`] (`install_diagnostics`).
    ///
    /// # Errors
    ///
    /// A delete or a pragma fails; the batches before it stay deleted.
    pub async fn prune(&self, settings: &RetentionSettings) -> Result<PruneReport> {
        let mut deleted = 0;
        for (table, limits) in settings.iter() {
            loop {
                let step = {
                    let mut writer = self.writer().await;
                    prune_step(&mut writer, table, limits, PRUNE_BATCH).await?
                };
                deleted += step.deleted;
                if !step.more {
                    break;
                }
                // Let whoever waits for the writer go first.
                tokio::task::yield_now().await;
            }
        }
        // Also without deletes: free pages a sweep cut short left behind are still given back.
        let freed_pages = self.reclaim().await?;
        Ok(PruneReport {
            deleted,
            freed_pages,
        })
    }

    /// Returns the free pages to the file system when there are many, chunk by chunk with the
    /// writer released between two, then empties the write-ahead log; the pages it returned.
    pub(crate) async fn reclaim(&self) -> Result<u64> {
        let mut freed = 0;
        loop {
            let returned = {
                let mut writer = self.writer().await;
                let free = freelist_count(&mut writer).await?;
                if free < VACUUM_FREE_PAGES {
                    break;
                }
                sqlx::query(VACUUM_CHUNK_SQL).execute(&mut *writer).await?;
                free - freelist_count(&mut writer).await?
            };
            if returned <= 0 {
                break;
            }
            freed += u64::try_from(returned).unwrap_or(0);
            // Let whoever waits for the writer go first.
            tokio::task::yield_now().await;
        }
        // Answers `busy` rather than failing while a reader holds an old snapshot; the next
        // checkpoint catches up.
        let mut writer = self.writer().await;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&mut *writer)
            .await?;
        Ok(freed)
    }
}

async fn freelist_count(connection: &mut SqliteConnection) -> Result<i64> {
    Ok(sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(connection)
        .await?)
}

/// At most `batch` rows of `table`: those older than the age limit, then the oldest over the
/// row limit.
///
/// Every statement walks an index from one end and stops after `batch` rows, or `max_rows` for
/// the cutoff: a batch costs the same in a table of five thousand rows and of forty million. A
/// `COUNT(*)` or an `ORDER BY id` under a time filter would read the whole table in every batch,
/// and the sweep of a flooded table would never finish while it held the writer.
async fn prune_step(
    connection: &mut SqliteConnection,
    table: &RetainedTable,
    limits: RetentionLimits,
    batch: u64,
) -> Result<Step> {
    let aged = sqlx::query(AssertSqlSafe(aged_sql(table)))
        .bind(format!("-{} days", limits.max_age_days))
        .bind(i64::try_from(batch).unwrap_or(i64::MAX))
        .execute(&mut *connection)
        .await?
        .rows_affected();

    let take = batch.saturating_sub(aged);
    let mut over = 0;
    if take > 0 {
        // The newest row past the limit; it and everything before it go.
        let cutoff: Option<i64> = sqlx::query_scalar(AssertSqlSafe(cutoff_sql(table)))
            .bind(i64::try_from(limits.max_rows).unwrap_or(i64::MAX))
            .fetch_optional(&mut *connection)
            .await?;
        if let Some(cutoff) = cutoff {
            over = sqlx::query(AssertSqlSafe(over_sql(table)))
                .bind(cutoff)
                .bind(i64::try_from(take).unwrap_or(i64::MAX))
                .execute(&mut *connection)
                .await?
                .rows_affected();
        }
    }
    Ok(Step {
        deleted: aged + over,
        more: aged == batch || (take > 0 && over == take),
    })
}

// Table and column are constants of `RETAINED_TABLES`, never input; the limits are bound.

/// Deletes up to `?2` rows older than `?1` (an SQLite date modifier), along the time index.
fn aged_sql(table: &RetainedTable) -> String {
    let (name, time) = (table.table, table.time_column);
    format!(
        "DELETE FROM {name} WHERE id IN (SELECT id FROM {name} \
         WHERE {time} < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?) ORDER BY {time} LIMIT ?)"
    )
}

/// The id of the newest row beyond the newest `?1`, if there are more.
fn cutoff_sql(table: &RetainedTable) -> String {
    let name = table.table;
    format!("SELECT id FROM {name} ORDER BY id DESC LIMIT 1 OFFSET ?")
}

/// Deletes up to `?2` of the oldest rows up to id `?1`, along the primary key.
fn over_sql(table: &RetainedTable) -> String {
    let name = table.table;
    format!(
        "DELETE FROM {name} WHERE id IN (SELECT id FROM {name} \
         WHERE id <= ? ORDER BY id LIMIT ?)"
    )
}

#[cfg(test)]
#[path = "prune_tests.rs"]
mod tests;
