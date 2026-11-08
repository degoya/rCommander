//! The most visited folders (`dir_frequent`, migration 0031, RC-100-92): the lower end of the
//! history list (Alt+Down), as Total Commander 11 shows its frequently used folders.
//!
//! One row per folder, its location the engine's `VfsPath` JSON, opaque here. A visit adds one to
//! the folder's count; every 30 days since a row was last halved its count is halved, on the way in
//! (a visit) and on the way out (a read), so a folder used often long ago fades without a sweep of
//! its own. The table is retained (`RETAINED_TABLES`): the shared sweep drops rows not visited
//! for long and, past the row limit, the oldest ones.

use sqlx::{Connection as _, Row as _};

use crate::{Db, Result};

/// The time after which a count is halved, in seconds: 30 days.
pub const FREQUENT_HALF_LIFE_SECS: i64 = 30 * 24 * 60 * 60;

/// The longest location a row takes (the table's check).
const MAX_LOCATION: usize = 8192;

/// One folder and its count, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrequentDir {
    /// The folder, as the engine's `VfsPath` JSON.
    pub location: String,
    /// The visits, halved for every 30 days since the count was last halved.
    pub visits: i64,
}

/// `visits` counted since `since` (unix seconds) as they stand at `now`: halved once for every
/// whole [`FREQUENT_HALF_LIFE_SECS`] in between, and the time from which the halved count runs.
#[must_use]
pub fn decayed(visits: i64, since: i64, now: i64) -> (i64, i64) {
    let periods = now.saturating_sub(since).max(0) / FREQUENT_HALF_LIFE_SECS;
    if periods == 0 {
        return (visits, since);
    }
    let halved = if periods >= 63 {
        0
    } else {
        visits.max(0) >> periods
    };
    (
        halved,
        since.saturating_add(periods.saturating_mul(FREQUENT_HALF_LIFE_SECS)),
    )
}

impl Db {
    /// Counts a visit of `location` at `now` (unix seconds): one more for a folder known, halved
    /// first as [`decayed`] says; a new row otherwise. A location longer than the table takes is
    /// not counted.
    ///
    /// # Errors
    ///
    /// The read or the write fails.
    pub async fn record_frequent(&self, location: &str, now: i64) -> Result<()> {
        if location.len() > MAX_LOCATION {
            return Ok(());
        }
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        let known =
            sqlx::query("SELECT visits, counted_since FROM dir_frequent WHERE location = ?")
                .bind(location)
                .fetch_optional(&mut *tx)
                .await?;
        match known {
            Some(row) => {
                let (visits, since) =
                    decayed(row.try_get("visits")?, row.try_get("counted_since")?, now);
                sqlx::query(
                    "UPDATE dir_frequent SET visits = ?, counted_since = ?, \
                     last_visit = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE location = ?",
                )
                .bind(visits.saturating_add(1))
                .bind(since)
                .bind(location)
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO dir_frequent (location, visits, counted_since) VALUES (?, 1, ?)",
                )
                .bind(location)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// The `limit` folders with the most visits at `now`, most first (equal counts: the one
    /// visited last first); folders whose count has faded to nothing are left out.
    ///
    /// # Errors
    ///
    /// The read fails.
    pub async fn frequent_dirs(&self, limit: usize, now: i64) -> Result<Vec<FrequentDir>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT location, visits, counted_since FROM dir_frequent ORDER BY last_visit DESC",
        )
        .fetch_all(self.readers())
        .await?;
        let mut dirs = Vec::with_capacity(rows.len());
        for row in &rows {
            let (visits, _) = decayed(row.try_get("visits")?, row.try_get("counted_since")?, now);
            if visits > 0 {
                dirs.push(FrequentDir {
                    location: row.try_get("location")?,
                    visits,
                });
            }
        }
        // Stable: of equal counts the one visited last stays first.
        dirs.sort_by_key(|a| std::cmp::Reverse(a.visits));
        dirs.truncate(limit);
        Ok(dirs)
    }
}

#[cfg(test)]
#[path = "dir_frequent_tests.rs"]
mod tests;
