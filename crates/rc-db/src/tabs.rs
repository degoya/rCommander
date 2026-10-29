//! The folder tabs of both panels and their folder history (`tabs`, `dir_history`, migration
//! 0008).
//!
//! The tabs are state the window leaves behind: [`Db::save_tabs`] replaces all of them, with
//! their history, in one transaction when the app ends; [`Db::load_tabs`] reads them at the next
//! start. Locations, sort and mode are the engine's JSON, opaque here; the engine checks them when
//! it reads them back. `dir_history` is a retained table (`RETAINED_TABLES`): a save inserts the
//! farthest steps of every tab first, so the sweep, which deletes by insertion order, takes the
//! oldest folders first.

use sqlx::{Connection as _, Row as _};

use crate::{Db, Result};

/// One tab as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTab {
    /// `left` or `right`.
    pub panel: String,
    /// Its place in the panel's tab row, from 0.
    pub position: i64,
    /// Whether it is the tab the panel shows.
    pub active: bool,
    pub locked: bool,
    /// The folder, as the engine's `VfsPath` JSON.
    pub location: String,
    /// The sort order, as the engine's JSON.
    pub sort: String,
    /// Full or brief, as the engine's JSON.
    pub mode: String,
    /// The folders behind and ahead of `location`.
    pub history: Vec<StoredVisit>,
}

/// One folder of a tab's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVisit {
    /// -1, -2, ... behind the tab's folder; 1, 2, ... ahead of it. Never 0.
    pub step: i64,
    /// The folder, as the engine's `VfsPath` JSON.
    pub location: String,
}

impl Db {
    /// Replaces every stored tab and its history with `tabs`, in one transaction. Visits with
    /// step 0 are skipped: the tab's own folder is its location.
    ///
    /// # Errors
    ///
    /// A write fails; nothing is replaced then.
    pub async fn save_tabs(&self, tabs: &[StoredTab]) -> Result<()> {
        let mut writer = self.writer().await;
        let mut tx = writer.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM dir_history")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM tabs").execute(&mut *tx).await?;
        let mut visits: Vec<(i64, &StoredVisit)> = Vec::new();
        for tab in tabs {
            let id = sqlx::query(
                "INSERT INTO tabs (panel, position, active, locked, location, sort, mode) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&tab.panel)
            .bind(tab.position)
            .bind(i64::from(tab.active))
            .bind(i64::from(tab.locked))
            .bind(&tab.location)
            .bind(&tab.sort)
            .bind(&tab.mode)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            visits.extend(
                tab.history
                    .iter()
                    .filter(|visit| visit.step != 0)
                    .map(|visit| (id, visit)),
            );
        }
        // The farthest steps first: the sweep deletes the lowest ids first.
        visits.sort_by_key(|(_, visit)| std::cmp::Reverse(visit.step.abs()));
        for (tab_id, visit) in visits {
            sqlx::query("INSERT INTO dir_history (tab_id, step, location) VALUES (?, ?, ?)")
                .bind(tab_id)
                .bind(visit.step)
                .bind(&visit.location)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The stored tabs, by panel and position, each with its history ordered by step.
    ///
    /// # Errors
    ///
    /// A read fails.
    pub async fn load_tabs(&self) -> Result<Vec<StoredTab>> {
        let mut snapshot = self.readers().begin().await?;
        let rows = sqlx::query(
            "SELECT id, panel, position, active, locked, location, sort, mode FROM tabs \
             ORDER BY panel, position, id",
        )
        .fetch_all(&mut *snapshot)
        .await?;
        let visits = sqlx::query("SELECT tab_id, step, location FROM dir_history ORDER BY step")
            .fetch_all(&mut *snapshot)
            .await?;
        snapshot.rollback().await?;
        let mut tabs = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: i64 = row.try_get("id")?;
            let history = visits
                .iter()
                .filter(|visit| visit.try_get::<i64, _>("tab_id").is_ok_and(|tab| tab == id))
                .map(|visit| {
                    Ok(StoredVisit {
                        step: visit.try_get("step")?,
                        location: visit.try_get("location")?,
                    })
                })
                .collect::<sqlx::Result<Vec<_>>>()?;
            tabs.push(StoredTab {
                panel: row.try_get("panel")?,
                position: row.try_get("position")?,
                active: row.try_get::<i64, _>("active")? != 0,
                locked: row.try_get::<i64, _>("locked")? != 0,
                location: row.try_get("location")?,
                sort: row.try_get("sort")?,
                mode: row.try_get("mode")?,
                history,
            });
        }
        Ok(tabs)
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;
