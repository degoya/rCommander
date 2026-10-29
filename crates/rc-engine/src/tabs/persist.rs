//! The tabs across a restart: [`Engine::save_tabs`] when the window closes,
//! [`Engine::restore_tabs`] when it opens.
//!
//! A stored tab whose folder is gone comes back on the nearest folder above it that is still
//! there, and the window says so ([`Relocated`]); a tab with nothing left above it, or a row that
//! does not read, is dropped. A tab inside an archive whose file is gone comes back on the folder
//! that held the archive (RC-030-02). A panel without a stored tab keeps the folder it started in.
//! The look for a tab's folder waits at most [`DETAIL_TIMEOUT`], as the drive bar waits for a
//! drive (RC-080-51): a share that does not answer does not hold the start, and its tab is
//! dropped as one whose folder is gone.

use rc_db::{StoredTab, StoredVisit};
use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use super::{
    Tab, TabId, TabSet, Tabs,
    commands::{split, tab_state},
    history::History,
};
use crate::{
    DETAIL_TIMEOUT, Engine, EngineError, EngineEvent, ListMode, PanelChange, PanelId, SortOrder,
};

/// A stored tab whose folder was gone at the start, and the folder it shows instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relocated {
    pub panel: PanelId,
    /// The folder the tab showed when the app ended.
    pub stored: VfsPath,
    /// The nearest folder above it that is still there.
    pub shown: VfsPath,
}

fn panel_name(panel: PanelId) -> &'static str {
    match panel {
        PanelId::Left => "left",
        PanelId::Right => "right",
    }
}

/// Every tab of both panels as stored rows.
fn stored_rows(tabs: &Tabs) -> Vec<StoredTab> {
    let mut rows = Vec::new();
    for panel in PanelId::BOTH {
        let set = tabs.set(panel);
        for (position, tab) in set.tabs().iter().enumerate() {
            // A folder that does not encode (a path that is not Unicode) is not kept.
            let Ok(location) = serde_json::to_string(&tab.path) else {
                continue;
            };
            let history = tab
                .history
                .steps()
                .filter_map(|(step, path)| {
                    let location = serde_json::to_string(path).ok()?;
                    Some(StoredVisit { step, location })
                })
                .collect();
            rows.push(StoredTab {
                panel: panel_name(panel).to_owned(),
                position: i64::try_from(position).unwrap_or(i64::MAX),
                active: position == set.active(),
                locked: tab.locked,
                location,
                sort: serde_json::to_string(&tab.sort).unwrap_or_default(),
                mode: serde_json::to_string(&tab.mode).unwrap_or_default(),
                history,
            });
        }
    }
    rows
}

/// A stored row as a tab, its id given later; `None` when its folder does not read.
fn tab_from_row(row: &StoredTab) -> Option<Tab> {
    let path: VfsPath = serde_json::from_str(&row.location).ok()?;
    let back = locations(row.history.iter().filter(|visit| visit.step < 0));
    let ahead = locations(row.history.iter().filter(|visit| visit.step > 0));
    Some(Tab {
        id: TabId(0),
        history: History::restore(back, path.clone(), ahead),
        path,
        sort: serde_json::from_str::<SortOrder>(&row.sort).unwrap_or_default(),
        mode: serde_json::from_str::<ListMode>(&row.mode).unwrap_or_default(),
        locked: row.locked,
        cursor: None,
    })
}

/// The folders of `visits` that read, in their order.
fn locations<'a>(visits: impl Iterator<Item = &'a StoredVisit>) -> Vec<VfsPath> {
    visits
        .filter_map(|visit| serde_json::from_str(&visit.location).ok())
        .collect()
}

impl Engine {
    /// The nearest folder at or above `path` that is a folder still; `None` when none is, or
    /// none answered within [`DETAIL_TIMEOUT`]. Each step asks its own backend: above an
    /// archive's root is the folder of the archive file.
    async fn nearest_folder(&self, path: &VfsPath) -> Option<VfsPath> {
        let deadline = tokio::time::Instant::now() + DETAIL_TIMEOUT;
        let mut candidate = Some(path.clone());
        while let Some(folder) = candidate {
            if let Ok(vfs) = self.vfs(&folder) {
                match tokio::time::timeout_at(deadline, vfs.stat(&folder.path)).await {
                    Ok(Ok(entry)) if entry.is_dir_like() => return Some(folder),
                    Ok(_) => {}
                    Err(_) => {
                        tracing::warn!(
                            "a stored tab's folder did not answer in time; not restored"
                        );
                        return None;
                    }
                }
            }
            candidate = folder.parent();
        }
        None
    }

    /// Stores the tabs of both panels, with their history, in place of the stored ones.
    ///
    /// # Errors
    ///
    /// No database is attached, or the write fails.
    pub async fn save_tabs(&self) -> Result<(), EngineError> {
        let rows = {
            let mut guard = self.state();
            let state = &mut *guard;
            for panel in PanelId::BOTH {
                let (live, tabs) = split(state, panel);
                tabs.sync(panel, live);
            }
            stored_rows(&state.tabs)
        };
        self.db()?.save_tabs(&rows).await?;
        Ok(())
    }

    /// Brings back the stored tabs of both panels; the tabs whose folder was gone and where
    /// they are now.
    ///
    /// # Errors
    ///
    /// No database is attached, or the read fails; the panels stay as they are.
    pub async fn restore_tabs(&self) -> Result<Vec<Relocated>, EngineError> {
        let stored = self.db()?.load_tabs().await?;
        let mut relocated = Vec::new();
        for panel in PanelId::BOTH {
            let rows: Vec<&StoredTab> = stored
                .iter()
                .filter(|row| row.panel == panel_name(panel))
                .collect();
            let mut tabs = Vec::with_capacity(rows.len());
            let mut active = 0;
            for row in rows {
                let Some(mut tab) = tab_from_row(row) else {
                    continue;
                };
                let Some(folder) = self.nearest_folder(&tab.path).await else {
                    continue;
                };
                if folder != tab.path {
                    relocated.push(Relocated {
                        panel,
                        stored: tab.path.clone(),
                        shown: folder.clone(),
                    });
                    tab.history.visit(&folder);
                    tab.path = folder;
                }
                if row.active {
                    active = tabs.len();
                }
                tabs.push(tab);
            }
            if let Some(set) = TabSet::from_tabs(tabs, active) {
                self.show_tab_set(panel, set).await?;
            }
        }
        Ok(relocated)
    }

    /// Panel `panel` takes `set` as its tabs and shows its active tab.
    pub(super) async fn show_tab_set(
        &self,
        panel: PanelId,
        set: TabSet,
    ) -> Result<(), EngineError> {
        let shown = set.active_tab().clone();
        let show_hidden = self.state().show_hidden;
        let (path, entries, archive) = self
            .list_nearest(&shown.path, shown.sort, show_hidden)
            .await?;
        {
            let mut guard = self.state();
            let state = &mut *guard;
            let (live, tabs) = split(state, panel);
            tabs.replace(panel, set);
            tabs.set_mut(panel).active_tab_mut().path.clone_from(&path);
            *live = tab_state(
                live,
                path,
                entries,
                (shown.sort, shown.mode),
                show_hidden,
                None,
            );
            live.archive = archive;
        }
        self.publish(EngineEvent::Panel {
            panel,
            change: PanelChange::Navigated,
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "persist_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "persist_hang_tests.rs"]
mod hang_tests;
