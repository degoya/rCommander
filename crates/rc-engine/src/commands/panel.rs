//! The `panel.*` commands.

use std::path::{Component, Path, PathBuf};

use rc_vfs::{BackendId, VfsPath};

use super::panel_listing::{Arriving, Placing};
use crate::{
    CursorTarget, Engine, EngineError, EngineEvent, Outcome, PanelChange, PanelId, PanelState,
    SortOrder,
    archives::entry_name,
    drives::drive_key,
    state::{by_name, list_sorted, unlisted},
};

impl Engine {
    /// A copy of a panel's state, entries included; the entries and the selection are shared,
    /// not copied (RC-080-45).
    #[must_use]
    pub fn panel(&self, id: PanelId) -> PanelState {
        self.state().panel(id).clone()
    }

    /// The directory a panel shows, without a copy of the rest of its state (RC-080-45).
    #[must_use]
    pub fn panel_path(&self, id: PanelId) -> VfsPath {
        self.state().panel(id).path.clone()
    }

    /// `panel.list_dir`: a directory's entries in the panel's order, hidden ones included; no
    /// panel moves. The directory may be an FTP or SFTP location (`ftp://<connection>/<path>`,
    /// `sftp://<connection>/<path>`).
    pub(crate) async fn list_dir(
        &self,
        panel: Option<PanelId>,
        target: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (current, sort) = {
            let state = self.state();
            let panel = state.panel(id);
            (panel.path.clone(), panel.sort)
        };
        let path = match target {
            Some(target) => self.locate(&current, target).await?,
            None => current,
        };
        let vfs = self.open_vfs(&path, self.announces_locked_vault()).await?;
        // Hidden entries too: each one says it is hidden.
        let entries = list_sorted(&vfs, &path, sort, true).await?;
        Ok(Outcome::Listing { path, entries })
    }

    /// `panel.navigate`: the panel shows `target`, resolved against its directory, or the
    /// location it writes out (`ftp://`, `sftp://`). A local path that does not list because it
    /// leads through an archive file (`C:\data\backup.zip\docs`, or the archive file itself)
    /// opens the archive there.
    pub(crate) async fn navigate(
        &self,
        panel: Option<PanelId>,
        target: &str,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let current = self.state().panel(id).path.clone();
        let target = self.locate(&current, target).await?;
        match self.show_path(id, target.clone()).await {
            Err(error @ EngineError::Vfs(_)) if target.backend == BackendId::Local => {
                match self.through_archive(&target).await {
                    Ok(Some(inside)) => self.show_path(id, inside).await,
                    _ => Err(error),
                }
            }
            shown => shown,
        }
    }

    /// The panel `id` shows `path`. Going up puts the cursor on the directory just left, as Total
    /// Commander does; the panel remembers `path` as its last directory on that drive, and its
    /// active tab records it in its history.
    pub(crate) async fn show_path(
        &self,
        id: PanelId,
        path: VfsPath,
    ) -> Result<Outcome, EngineError> {
        let (current, sort, show_hidden) = self.listing_of(id);
        // A locked tab keeps its folder; another one opens in a new tab (RC-010-03).
        let locked = self.state().tabs.locked(id);
        if locked && path != current {
            return self.open_tab_at(id, path).await;
        }
        let vfs = self.open_vfs(&path, self.announces_locked_vault()).await?;
        // Listed before the lock is taken: the state never waits on a file system. A listing
        // that runs long is shown with what arrived, the rest follows (`panel_listing.rs`).
        let (entries, arriving) = self.first_entries(&vfs, &path, sort, show_hidden).await?;
        let archive = self.archive_info(&path).await;
        let drive = drive_key(&path, &self.inner.drives.roots());
        // Going up puts the cursor on the folder just left (an archive left at its root: the
        // archive file).
        let wanted = if current.parent().as_ref() == Some(&path) {
            entry_name(&current)
        } else {
            None
        };
        let (snapshot, placed) = {
            let mut state = self.state();
            if let Some(drive) = drive {
                state.last_dirs[id.index()].insert(drive, path.clone());
            }
            let active = state.active == id;
            let panel = state.panel_mut(id);
            let mut next = panel.next(path, entries, sort, show_hidden);
            next.archive = archive;
            next.listing = arriving.as_ref().map(Arriving::run);
            if let Some(index) = wanted.as_deref().and_then(|name| next.index_of(name)) {
                next.cursor = index;
            }
            *panel = next;
            let placed = panel.cursor_entry().map(|entry| entry.name.clone());
            let snapshot = panel.snapshot(id, active);
            state.tabs.visited(id, &snapshot.path);
            (snapshot, placed)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Navigated,
        });
        if let Some(arriving) = arriving {
            arriving.follow(self.clone(), id, Placing { wanted, placed });
        }
        // A panel that shows its terminal takes the shell along (RC-040-04).
        self.terminal_follows_panel(id).await;
        Ok(Outcome::Panel(snapshot))
    }

    /// The `panel.refresh` command (Ctrl+R, F2): a flat view (Ctrl+B, find results) is left for
    /// the folder it was made from (owner, 2026-10-18: Ctrl+R brings the folders back); any other
    /// panel is read again ([`Engine::refresh`], which a changed view re-reads flat views with).
    pub(crate) async fn refresh_command(
        &self,
        panel: Option<PanelId>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        match self.flat_root(id) {
            Some(root) => self.show_path(id, root).await,
            None => self.refresh(Some(id)).await,
        }
    }

    /// `panel.refresh`: the panel's directory read again. The cursor stays on its entry, or at
    /// its place when the entry is gone; selected names that are still there stay selected.
    pub(crate) async fn refresh(&self, panel: Option<PanelId>) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        // A flat view is read again as a whole (`flat.rs`).
        if let Some(outcome) = self.refresh_flat(id) {
            return outcome;
        }
        let (path, sort, show_hidden) = self.listing_of(id);
        let vfs = self.open_vfs(&path, self.announces_locked_vault()).await?;
        let entries = list_sorted(&vfs, &path, sort, show_hidden).await?;
        let archive = self.archive_info(&path).await;
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            // A panel that moved on while the listing ran keeps where it went.
            if panel.path == path {
                let mut next = panel.next(path, entries, sort, show_hidden);
                next.archive = archive;
                // One index for the cursor and every selected name: the folder is not searched
                // once per name (RC-080-45).
                let listed = by_name(&next.entries);
                let kept = panel
                    .cursor_entry()
                    .and_then(|entry| listed.get(entry.name.as_str()).copied());
                next.cursor =
                    kept.unwrap_or_else(|| panel.cursor.min(next.entries.len().saturating_sub(1)));
                next.selection = panel.selection.still_listed(&listed);
                *panel = next;
            }
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Navigated,
        });
        Ok(Outcome::Panel(snapshot))
    }

    /// What a listing of panel `id` needs: its directory, its order and whether hidden entries
    /// are shown.
    fn listing_of(&self, id: PanelId) -> (VfsPath, SortOrder, bool) {
        let state = self.state();
        let panel = state.panel(id);
        (panel.path.clone(), panel.sort, state.show_hidden)
    }

    /// `panel.set_cursor`.
    pub(crate) fn set_cursor(
        &self,
        panel: Option<PanelId>,
        target: CursorTarget,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            panel.cursor = match target {
                CursorTarget::Index(index) if index < panel.entries.len() => index,
                CursorTarget::Index(index) => {
                    return Err(EngineError::CursorOutOfRange {
                        index,
                        len: panel.entries.len(),
                    });
                }
                CursorTarget::Name(name) => panel
                    .index_of(&name)
                    .ok_or(EngineError::EntryNotFound { name })?,
            };
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Cursor,
        });
        Ok(Outcome::Panel(snapshot))
    }

    /// `panel.toggle_selection`: every name flips; one unknown name changes nothing.
    pub(crate) fn toggle_selection(
        &self,
        panel: Option<PanelId>,
        names: &[String],
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let snapshot = {
            let mut state = self.state();
            let active = state.active == id;
            let panel = state.panel_mut(id);
            if let Some(name) = unlisted(&panel.entries, names) {
                return Err(EngineError::EntryNotFound { name: name.clone() });
            }
            for name in names {
                if !panel.selection.remove(name) {
                    panel.selection.insert(name.clone());
                }
            }
            panel.snapshot(id, active)
        };
        self.publish(EngineEvent::Panel {
            panel: id,
            change: PanelChange::Selection,
        });
        Ok(Outcome::Panel(snapshot))
    }

    /// `panel.switch_active`: `panel`, or the other one.
    pub(crate) fn switch_active(&self, panel: Option<PanelId>) -> Outcome {
        let snapshot = {
            let mut state = self.state();
            let id = panel.unwrap_or_else(|| state.active.other());
            state.active = id;
            state.panel(id).snapshot(id, true)
        };
        self.publish(EngineEvent::ActivePanel {
            panel: snapshot.panel,
        });
        Outcome::Panel(snapshot)
    }
}

/// `target` against `current`: an absolute path as it is, a relative one joined, then `.` and
/// `..` folded lexically (`..` at the root stays at the root). On Windows a path with a root but
/// no drive (`\` or `/dir`) stays on the current drive or share, as Ctrl+\ does in Total
/// Commander: joining keeps the prefix.
///
/// Inside an archive a path with a root but no drive (`/docs`, `\`) starts at the archive's
/// root, `..` at that root leaves the archive into the folder holding it, and a path with a
/// drive or a share (`C:\data`) names a place outside the archive. On a server (SFTP, FTP) a
/// path with a drive or a share is a local one too: F5 from a server panel into `D:\` or
/// `\\nas\share` copies to this machine, never to a server path of that name.
pub(crate) fn resolve_path(current: &VfsPath, target: &str) -> VfsPath {
    let target = Path::new(target);
    let names_a_drive = matches!(target.components().next(), Some(Component::Prefix(_)));
    let base = match &current.backend {
        BackendId::Archive { outer, .. } if names_a_drive => outermost(outer),
        BackendId::Sftp { .. } | BackendId::Ftp { .. } if names_a_drive => VfsPath {
            backend: BackendId::Local,
            path: PathBuf::new(),
        },
        _ => current.clone(),
    };
    let joined = if target.is_absolute() {
        target.to_path_buf()
    } else {
        base.path.join(target)
    };
    let mut folded = VfsPath {
        backend: base.backend,
        path: PathBuf::new(),
    };
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    folded.path.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    folded.path.pop();
                } else if matches!(folded.backend, BackendId::Archive { .. })
                    && let Some(outside) = folded.parent()
                {
                    folded = outside;
                }
            }
            other => folded.path.push(other),
        }
    }
    folded
}

/// The location of the archive file `outer` on a backend that is no archive.
fn outermost(outer: &VfsPath) -> VfsPath {
    match &outer.backend {
        BackendId::Archive { outer, .. } => outermost(outer),
        _ => outer.clone(),
    }
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
