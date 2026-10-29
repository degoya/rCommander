//! The `tabs.*` commands: open, close, select, step through, lock, copy and move tabs.
//!
//! A folder is listed before the state lock is taken, as `panel.navigate` does; the tab a
//! command names is found again by its id afterwards, so a command that ran in between only
//! costs a `tab_not_found`.

use std::sync::Arc;

use rc_vfs::{Entry, VfsPath};

use super::{
    Tab, TabId, TabView, Tabs, TabsError,
    requests::{LockTabRequest, MoveTabRequest, NextTabRequest, OpenTabRequest, TabRequest},
};
use crate::{
    ArchiveInfo, Engine, EngineError, EngineEvent, ListMode, Outcome, PanelChange, PanelId,
    PanelState, SortOrder, State, commands::resolve_path, drives::drive_key, state::list_sorted,
};

/// The panel `id` and the tabs, borrowed apart.
pub(super) fn split(state: &mut State, id: PanelId) -> (&mut PanelState, &mut Tabs) {
    let panel = match id {
        PanelId::Left => &mut state.left,
        PanelId::Right => &mut state.right,
    };
    (panel, &mut state.tabs)
}

/// The state of a panel that shows `path` in another tab: view and remembered selection stay
/// the panel's, sort and mode are the tab's (`entries` are listed in `sort` order), the cursor
/// returns to `cursor` when that entry is still there.
pub(super) fn tab_state(
    previous: &PanelState,
    path: VfsPath,
    entries: Vec<Entry>,
    (sort, mode): (SortOrder, ListMode),
    show_hidden: bool,
    cursor: Option<&str>,
) -> PanelState {
    let mut next = PanelState::showing(path, entries);
    next.view = previous.view;
    next.sort = sort;
    next.mode = mode;
    next.show_hidden = show_hidden;
    next.remembered = previous.remembered.clone();
    if let Some(index) = cursor.and_then(|name| next.index_of(name)) {
        next.cursor = index;
    }
    next
}

impl Engine {
    /// `tabs.list`.
    pub(crate) fn list_tabs(&self, panel: Option<PanelId>) -> Outcome {
        self.tabs_outcome(self.resolve(panel))
    }

    /// The tabs of panel `id` as commands answer them.
    pub(crate) fn tabs_outcome(&self, id: PanelId) -> Outcome {
        Outcome::Tabs {
            panel: id,
            tabs: self.tabs(id),
        }
    }

    /// A panel's tabs, left to right, as `tabs.list` answers them: what the window draws.
    #[must_use]
    pub fn tabs(&self, panel: PanelId) -> Vec<TabView> {
        let state = self.state();
        state.tabs.views(panel, state.panel(panel))
    }

    /// The folder under the cursor of panel `id`, or the panel's own when the cursor is on a
    /// file or the folder is empty.
    pub(crate) fn cursor_dir(&self, id: PanelId) -> VfsPath {
        let state = self.state();
        let panel = state.panel(id);
        match panel.cursor_entry().filter(|entry| entry.is_dir_like()) {
            Some(entry) => resolve_path(&panel.path, &entry.name),
            None => panel.path.clone(),
        }
    }

    /// `tabs.open`: a new tab after the active one, which it becomes.
    pub(crate) async fn open_tab(&self, request: OpenTabRequest) -> Result<Outcome, EngineError> {
        let source = self.resolve(request.panel);
        let path = match request.path {
            Some(path) => {
                let current = self.state().panel(source).path.clone();
                resolve_path(&current, &path)
            }
            None if request.cursor => self.cursor_dir(source),
            None => self.state().panel(source).path.clone(),
        };
        let target = if request.other {
            source.other()
        } else {
            source
        };
        self.open_tab_at(target, path).await
    }

    /// Panel `id` shows `path` in a new tab after its active one, with the active tab's sort
    /// and mode. A locked tab sends every new folder here.
    pub(crate) async fn open_tab_at(
        &self,
        id: PanelId,
        path: VfsPath,
    ) -> Result<Outcome, EngineError> {
        let (sort, mode, show_hidden) = {
            let state = self.state();
            let panel = state.panel(id);
            (panel.sort, panel.mode, state.show_hidden)
        };
        let vfs = self.vfs(&path)?;
        let entries = list_sorted(&vfs, &path, sort, show_hidden).await?;
        let archive = self.archive_info(&path).await;
        let drive = drive_key(&path, &self.inner.drives.roots());
        {
            let mut guard = self.state();
            let state = &mut *guard;
            if let Some(drive) = drive {
                state.last_dirs[id.index()].insert(drive, path.clone());
            }
            let (panel, tabs) = split(state, id);
            tabs.sync(id, panel);
            let tab = tabs.new_tab(path.clone(), sort, mode);
            tabs.insert_active(id, tab);
            *panel = tab_state(panel, path, entries, (sort, mode), show_hidden, None);
            panel.archive = archive;
        }
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Navigated,
        });
        Ok(self.tabs_outcome(id))
    }

    /// `tabs.select`: the panel shows tab `tab`, in its folder or, when that is gone, the
    /// nearest folder above it that is still there.
    pub(crate) async fn select_tab(
        &self,
        panel: Option<PanelId>,
        tab: TabId,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (target, show_hidden) = {
            let state = self.state();
            let set = state.tabs.set(id);
            let index = set.index_of(tab).ok_or(TabsError::TabNotFound { tab })?;
            if index == set.active() {
                (None, state.show_hidden)
            } else {
                (Some(set.tabs()[index].clone()), state.show_hidden)
            }
        };
        let Some(target) = target else {
            return Ok(self.tabs_outcome(id));
        };
        let (path, entries, archive) = self
            .list_nearest(&target.path, target.sort, show_hidden)
            .await?;
        {
            let mut guard = self.state();
            let state = &mut *guard;
            let (panel, tabs) = split(state, id);
            let index = tabs
                .set(id)
                .index_of(tab)
                .ok_or(TabsError::TabNotFound { tab })?;
            tabs.sync(id, panel);
            tabs.activate(id, index);
            if path != target.path {
                tabs.visited(id, &path);
            }
            let shown = tabs.set_mut(id).active_tab_mut();
            shown.path.clone_from(&path);
            let listing = (shown.sort, shown.mode);
            let cursor = shown.cursor.take();
            *panel = tab_state(
                panel,
                path,
                entries,
                listing,
                show_hidden,
                cursor.as_deref(),
            );
            panel.archive = archive;
        }
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Navigated,
        });
        Ok(self.tabs_outcome(id))
    }

    /// `tabs.next`: the tab right of the active one (left with `backwards`), round at the ends.
    pub(crate) async fn next_tab(&self, request: NextTabRequest) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        let next = {
            let state = self.state();
            let set = state.tabs.set(id);
            let count = set.tabs().len();
            let index = if request.backwards {
                (set.active() + count - 1) % count
            } else {
                (set.active() + 1) % count
            };
            set.tabs()[index].id
        };
        self.select_tab(Some(id), next).await
    }

    /// `tabs.close`: the last tab of a panel stays. Closing the active tab shows its right
    /// neighbour, or its left one at the end.
    pub(crate) async fn close_tab(&self, request: TabRequest) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        let (closing, neighbour) = {
            let state = self.state();
            let set = state.tabs.set(id);
            if set.tabs().len() < 2 {
                return Err(TabsError::LastTab.into());
            }
            let index = match request.tab {
                Some(tab) => set.index_of(tab).ok_or(TabsError::TabNotFound { tab })?,
                None => set.active(),
            };
            let neighbour = (index == set.active()).then(|| {
                let at = if index + 1 < set.tabs().len() {
                    index + 1
                } else {
                    index - 1
                };
                set.tabs()[at].id
            });
            (set.tabs()[index].id, neighbour)
        };
        if let Some(neighbour) = neighbour {
            self.select_tab(Some(id), neighbour).await?;
        }
        {
            let mut state = self.state();
            let index = state
                .tabs
                .set(id)
                .index_of(closing)
                .ok_or(TabsError::TabNotFound { tab: closing })?;
            state.tabs.remove(id, index).ok_or(TabsError::LastTab)?;
        }
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Tabs,
        });
        Ok(self.tabs_outcome(id))
    }

    /// `tabs.lock`.
    pub(crate) fn lock_tab(&self, request: LockTabRequest) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        {
            let mut state = self.state();
            let set = state.tabs.set_mut(id);
            let index = match request.tab {
                Some(tab) => set.index_of(tab).ok_or(TabsError::TabNotFound { tab })?,
                None => set.active(),
            };
            let tab: &mut Tab = &mut set.tabs_mut()[index];
            tab.locked = request.locked.unwrap_or(!tab.locked);
        }
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Tabs,
        });
        Ok(self.tabs_outcome(id))
    }

    /// `tabs.copy_to_other`: a copy of the tab, unlocked, after the other panel's last tab; the
    /// other panel keeps showing its own.
    pub(crate) fn copy_tab_to_other(&self, request: TabRequest) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        let other = id.other();
        {
            let mut guard = self.state();
            let state = &mut *guard;
            let (panel, tabs) = split(state, id);
            tabs.sync(id, panel);
            let set = tabs.set(id);
            let index = match request.tab {
                Some(tab) => set.index_of(tab).ok_or(TabsError::TabNotFound { tab })?,
                None => set.active(),
            };
            let source = set.tabs()[index].clone();
            let copy = tabs.copy_tab(&source);
            tabs.push(other, copy);
        }
        self.publish(EngineEvent::Panel {
            panel: other,
            change: PanelChange::Tabs,
        });
        Ok(self.tabs_outcome(other))
    }

    /// `tabs.move`: the tab takes another place in its row, as a drag in the tab bar puts it;
    /// the panel keeps showing the tab it showed. The new order is stored with the tabs.
    pub(crate) fn move_tab(&self, request: MoveTabRequest) -> Result<Outcome, EngineError> {
        let id = self.resolve(request.panel);
        let tab = request.tab;
        self.state()
            .tabs
            .move_tab(id, tab, request.to)
            .ok_or(TabsError::TabNotFound { tab })?;
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Tabs,
        });
        Ok(self.tabs_outcome(id))
    }

    /// `path` listed in `sort` order, or else the nearest folder above it that lists: the folder
    /// listed, its entries and, inside an archive, what the archive says about them. Each step
    /// asks its own backend: above an archive's root is the folder of the archive file. The
    /// first failure is the error when nothing above lists either.
    pub(crate) async fn list_nearest(
        &self,
        path: &VfsPath,
        sort: SortOrder,
        show_hidden: bool,
    ) -> Result<(VfsPath, Vec<Entry>, Option<Arc<ArchiveInfo>>), EngineError> {
        let mut first = None;
        let mut candidate = Some(path.clone());
        while let Some(folder) = candidate {
            let listed = match self.vfs(&folder) {
                Ok(vfs) => list_sorted(&vfs, &folder, sort, show_hidden).await,
                Err(error) => Err(error),
            };
            match listed {
                Ok(entries) => {
                    let archive = self.archive_info(&folder).await;
                    return Ok((folder, entries, archive));
                }
                Err(error) => {
                    if first.is_none() {
                        first = Some(error);
                    }
                    candidate = folder.parent();
                }
            }
        }
        Err(first.unwrap_or(EngineError::BackendUnavailable))
    }
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
