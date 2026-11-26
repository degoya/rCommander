//! The tabs across a restart: [`Engine::save_tabs`] when the window closes,
//! [`Engine::restore_tabs`] when it opens.
//!
//! A stored tab whose folder is gone comes back on the nearest folder above it that is still
//! there, and the window says so ([`Relocated`]); a tab with nothing left above it, or a row that
//! does not read, is dropped. A tab inside an archive whose file is gone comes back on the folder
//! that held the archive (RC-030-02). A panel without a stored tab keeps the folder it started in.
//! The look for a tab's folder waits at most [`DETAIL_TIMEOUT`], as the drive bar waits for a
//! drive (RC-080-51): a share that does not answer does not hold the start, and its tab is
//! dropped as one whose folder is gone. All tabs are looked for at once, under one deadline, so
//! ten tabs on a dead share hold the start no longer than one (RC-110-64).

use rc_db::{StoredTab, StoredVisit};
use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use super::{
    Tab, TabId, TabSet, Tabs,
    commands::{split, tab_state},
    history::History,
    rename::checked_title,
};
use crate::file_filter::FileFilter;
use crate::recover::LogPanic as _;
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

/// The stored tabs with their folders looked for ([`Engine::look_for_tabs`]), for
/// [`Engine::show_looked_tabs`]: the start looks for them while it looks at the start folders.
#[derive(Debug)]
pub struct LookedTabs {
    /// Each readable stored tab in its stored order: its panel, whether it was active, the tab,
    /// and the folder it shows now - `None` when none answered.
    tabs: Vec<(PanelId, bool, Tab, Option<VfsPath>)>,
}

impl LookedTabs {
    /// Whether `panel` gets at least one tab back.
    #[must_use]
    pub fn restores(&self, panel: PanelId) -> bool {
        self.tabs
            .iter()
            .any(|(tab_panel, _, _, folder)| *tab_panel == panel && folder.is_some())
    }
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
                title: tab.title.clone(),
                file_filter: tab.file_filter.stored(),
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
        // Read back as untrusted: a name that no longer fits is dropped.
        title: row
            .title
            .as_deref()
            .and_then(|title| checked_title(title).ok().flatten()),
        file_filter: FileFilter::from_stored(&row.file_filter),
    })
}

/// The folders of `visits` that read, in their order.
fn locations<'a>(visits: impl Iterator<Item = &'a StoredVisit>) -> Vec<VfsPath> {
    visits
        .filter_map(|visit| serde_json::from_str(&visit.location).ok())
        .collect()
}

impl Engine {
    /// The nearest folder at or above each of `paths`, in their order: all looked for at once,
    /// each given up at the one deadline [`DETAIL_TIMEOUT`] from now.
    async fn nearest_folders(&self, paths: Vec<VfsPath>) -> Vec<Option<VfsPath>> {
        let deadline = tokio::time::Instant::now() + DETAIL_TIMEOUT;
        let looks: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let engine = self.clone();
                tokio::spawn(async move { engine.nearest_folder(&path, deadline).await })
            })
            .collect();
        let mut folders = Vec::with_capacity(looks.len());
        // Each look ends by the deadline, so waiting for them in turn waits for the slowest.
        for look in looks {
            folders.push(look.await.log_panic("engine.tab_folder").ok().flatten());
        }
        folders
    }

    /// The nearest folder at or above `path` that is a folder still; `None` when none is, or
    /// none answered by `deadline`. Each step asks its own backend: above an archive's root is
    /// the folder of the archive file.
    async fn nearest_folder(
        &self,
        path: &VfsPath,
        deadline: tokio::time::Instant,
    ) -> Option<VfsPath> {
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
        let looked = self.look_for_tabs().await?;
        self.show_looked_tabs(looked).await
    }

    /// The first half of [`Engine::restore_tabs`]: the stored tabs read and their folders looked
    /// for, all at once under one deadline; nothing shown yet.
    ///
    /// # Errors
    ///
    /// No database is attached, or the read fails.
    pub async fn look_for_tabs(&self) -> Result<LookedTabs, EngineError> {
        let stored = self.db()?.load_tabs().await?;
        let read: Vec<(PanelId, bool, Tab)> = stored
            .iter()
            .filter_map(|row| {
                let panel = PanelId::BOTH
                    .into_iter()
                    .find(|panel| row.panel == panel_name(*panel))?;
                Some((panel, row.active, tab_from_row(row)?))
            })
            .collect();
        let folders = self
            .nearest_folders(read.iter().map(|(_, _, tab)| tab.path.clone()).collect())
            .await;
        let tabs = read
            .into_iter()
            .zip(folders)
            .map(|((panel, active, tab), folder)| (panel, active, tab, folder))
            .collect();
        Ok(LookedTabs { tabs })
    }

    /// The second half of [`Engine::restore_tabs`]: the tabs [`Engine::look_for_tabs`] found
    /// shown; the tabs whose folder was gone and where they are now. A panel without a tab
    /// found keeps what it shows.
    ///
    /// # Errors
    ///
    /// A panel's active tab does not list; the panels after it stay as they are.
    pub async fn show_looked_tabs(
        &self,
        looked: LookedTabs,
    ) -> Result<Vec<Relocated>, EngineError> {
        let mut checked = looked.tabs;
        let mut relocated = Vec::new();
        for panel in PanelId::BOTH {
            let mut tabs = Vec::new();
            let mut active = 0;
            for (_, is_active, mut tab, folder) in
                checked.extract_if(.., |(tab_panel, _, _, _)| *tab_panel == panel)
            {
                let Some(folder) = folder else {
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
                if is_active {
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
    pub(crate) async fn show_tab_set(
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
            live.set_file_filter(shown.file_filter.clone());
            live.archive = archive;
        }
        self.publish(EngineEvent::Panel {
            panel,
            change: PanelChange::Navigated,
        });
        self.columns_follow(panel, false).await;
        Ok(())
    }
}

#[cfg(test)]
#[path = "persist_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "persist_hang_tests.rs"]
mod hang_tests;
