//! The schedule (`inventory.refresh_minutes`, RC-100-39): every source synced before is synced
//! again once its last sync is that many minutes old; 0 syncs only when the person asks. A source
//! never synced waits for its preview in the window. A failed sync is recorded with the source
//! and waits for the next turn like a good one, so a server that is down is not asked in a loop.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::InventorySource;
use crate::{
    Engine,
    settings::{INVENTORY_REFRESH_MINUTES_KEY, stored_setting},
};

/// How often the schedule looks whether a source is due.
pub const SCHEDULE_TICK: Duration = Duration::from_secs(60);

impl Engine {
    /// The minutes between two syncs of a source; 0 for none.
    pub async fn inventory_refresh_minutes(&self) -> u64 {
        let Ok(db) = self.db() else {
            return 0;
        };
        stored_setting(db, INVENTORY_REFRESH_MINUTES_KEY)
            .await
            .as_ref()
            .and_then(Value::as_u64)
            .unwrap_or(0)
    }

    /// Syncs every source that is due now; what each sync did is logged, and a failure is
    /// recorded with its source. Called by the window every [`SCHEDULE_TICK`]; answers how many
    /// sources it synced.
    pub async fn run_due_inventories(&self) -> usize {
        let minutes = self.inventory_refresh_minutes().await;
        if minutes == 0 {
            return 0;
        }
        let Ok(sources) = self.db().map(|db| db.inventory_sources()) else {
            return 0;
        };
        let Ok(sources) = sources.await else {
            return 0;
        };
        let now = Utc::now();
        let mut synced = 0;
        for source in sources.iter().filter(|source| due(source, minutes, now)) {
            match self.refresh_inventory(source.id, false).await {
                Ok(_) => synced += 1,
                Err(error) => {
                    tracing::warn!(source = %source.id, code = error.code(), "a scheduled inventory sync failed");
                }
            }
        }
        synced
    }
}

/// Whether `source` is due at `now`: synced before, and at least `minutes` ago.
pub(crate) fn due(source: &InventorySource, minutes: u64, now: DateTime<Utc>) -> bool {
    let Some(last) = source
        .last_sync_at
        .as_deref()
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
    else {
        return false;
    };
    let minutes = i64::try_from(minutes).unwrap_or(i64::MAX);
    now.signed_duration_since(last).num_minutes() >= minutes
}
