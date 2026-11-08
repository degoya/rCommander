//! The most visited folders (RC-100-92), as Total Commander 11 shows its frequently used folders
//! at the lower end of the history list (Alt+Down).
//!
//! Every folder a panel shows is counted in `dir_frequent` (`rc_db::Db::record_frequent`), in a
//! task of its own so a listing never waits on the database; the counts halve every 30 days. The
//! `history.frequent_shown` most visited are kept in the engine's state after each visit, so the
//! window draws them without waiting; 0 shows none and counts nothing, as Total Commander's 0/0.

use std::time::{SystemTime, UNIX_EPOCH};

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::TabsError;
use crate::{Engine, EngineError, Outcome, PanelId, settings::stored_setting};

/// How many of the most visited folders the history list shows; 0 counts nothing.
pub const FREQUENT_SHOWN_KEY: &str = "history.frequent_shown";

/// A folder and how often it was shown, halved every 30 days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrequentFolder {
    pub path: VfsPath,
    pub visits: i64,
}

/// Now, in unix seconds.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

impl Engine {
    /// How many of the most visited folders are shown; 0 without a database.
    async fn frequent_shown(&self) -> usize {
        let Ok(db) = self.db() else {
            return 0;
        };
        stored_setting(db, FREQUENT_SHOWN_KEY)
            .await
            .as_ref()
            .and_then(Value::as_u64)
            .and_then(|shown| usize::try_from(shown).ok())
            .unwrap_or(0)
    }

    /// Counts a visit of `path` and reads the most visited again, in a task of its own; nothing
    /// while `history.frequent_shown` is 0.
    pub(crate) fn count_visit(&self, path: &VfsPath) {
        if self.db().is_err() {
            return;
        }
        let Ok(location) = serde_json::to_string(path) else {
            return;
        };
        let engine = self.clone();
        tokio::spawn(async move {
            let shown = engine.frequent_shown().await;
            if shown == 0 {
                return;
            }
            if let Ok(db) = engine.db()
                && let Err(error) = db.record_frequent(&location, now()).await
            {
                tracing::warn!(%error, "a folder visit was not counted");
                return;
            }
            engine.read_frequent_with(shown).await;
        });
    }

    /// The most visited folders read into the state, as `history.frequent_shown` says: at the
    /// start and after each visit.
    pub(crate) async fn read_frequent(&self) {
        let shown = self.frequent_shown().await;
        self.read_frequent_with(shown).await;
    }

    async fn read_frequent_with(&self, shown: usize) {
        let folders = match self.db() {
            Ok(db) if shown > 0 => match db.frequent_dirs(shown, now()).await {
                Ok(dirs) => dirs
                    .into_iter()
                    .filter_map(|dir| {
                        let path = serde_json::from_str(&dir.location).ok()?;
                        Some(FrequentFolder {
                            path,
                            visits: dir.visits,
                        })
                    })
                    .collect(),
                Err(error) => {
                    tracing::warn!(%error, "the most visited folders were not read");
                    return;
                }
            },
            _ => Vec::new(),
        };
        self.state().tabs.set_frequent(folders);
    }

    /// The most visited folders, most first, as the history list shows them.
    #[must_use]
    pub fn frequent(&self) -> Vec<FrequentFolder> {
        self.state().tabs.frequent().to_vec()
    }

    /// `history.open_frequent`: the panel shows the folder at `index` of the most visited ones
    /// (a locked tab opens it in a new tab).
    pub(crate) async fn history_open_frequent(
        &self,
        panel: Option<PanelId>,
        index: usize,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let target = {
            let state = self.state();
            let frequent = state.tabs.frequent();
            frequent
                .get(index)
                .map(|folder| folder.path.clone())
                .ok_or(TabsError::HistoryOutOfRange {
                    index,
                    len: frequent.len(),
                })?
        };
        self.show_path(id, target).await
    }
}

#[cfg(test)]
#[path = "frequent_tests.rs"]
mod tests;
