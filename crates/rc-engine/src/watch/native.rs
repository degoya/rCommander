//! The system's change notices (RC-110-02) through `notify`: `ReadDirectoryChangesW` on
//! Windows, inotify on Linux, FSEvents on macOS - each folder watched alone, never its
//! sub-folders.
//!
//! `notify` calls the handler on a thread of its own; the handler sorts a notice
//! ([`change_of`]) and passes it into the bounded channel of the watcher's loop without
//! waiting. When the channel is full, or the system says it lost notices (`Flag::Rescan`), the
//! overflow mark is set instead and the loop reads every watched panel once: memory stays
//! bounded in any flood.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _, event::ModifyKind,
};
use tokio::sync::mpsc;

use super::Signal;

/// What a notice means for a panel showing the folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    /// An entry was deleted or renamed: always read.
    Structure,
    /// An entry was created: read like [`Change::Structure`] unless the panel lists the name
    /// already, then like [`Change::Content`]. macOS's FSEvents keeps a path's flags together,
    /// so a write to a file created a while ago still arrives as a creation ([`brings_new`]).
    Created,
    /// An entry's size, time or attributes changed: read only with `panels.watch_changes`.
    Content,
}

/// What the notice `kind` means; `None` for one that changes nothing shown (an access).
pub(crate) fn change_of(kind: &EventKind) -> Option<Change> {
    match kind {
        EventKind::Create(_) => Some(Change::Created),
        EventKind::Remove(_)
        | EventKind::Modify(ModifyKind::Name(_))
        // A notice the platform cannot say more about may be anything.
        | EventKind::Any => Some(Change::Structure),
        EventKind::Modify(_) => Some(Change::Content),
        EventKind::Access(_) | EventKind::Other => None,
    }
}

/// What the handler passes on for `result`; `None` for nothing.
pub(crate) fn signal_of(result: notify::Result<Event>) -> Option<Signal> {
    match result {
        Ok(event) if event.need_rescan() => Some(Signal::Rescan),
        Ok(event) => change_of(&event.kind).map(|change| Signal::Changed {
            paths: event.paths,
            change,
        }),
        Err(error) => {
            tracing::debug!(%error, "the file watcher lost track; every watched panel is read");
            Some(Signal::Rescan)
        }
    }
}

/// Whether a notice about `changed` concerns the folder `watched`: an entry in it, or the
/// folder itself.
pub(crate) fn concerns(watched: &Path, changed: &Path) -> bool {
    changed == watched || changed.parent() == Some(watched)
}

/// Whether a creation notice about `changed` brings the folder `watched` an entry the panel
/// does not list yet; `listed` says whether a name is listed. The folder itself, or a name that
/// is no text, counts as new: a reading too many, never one too few.
pub(crate) fn brings_new(watched: &Path, changed: &Path, listed: impl Fn(&str) -> bool) -> bool {
    if changed == watched {
        return true;
    }
    changed
        .file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| !listed(name))
}

/// The path the system names a folder's changes by: macOS's FSEvents reports the real path
/// (`/private/var/...` for `/var/...`), the others the path as it was watched.
pub(crate) fn watch_key(path: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// The system watcher and the folders it watches.
pub(crate) struct Native {
    watcher: RecommendedWatcher,
    watched: BTreeSet<PathBuf>,
}

impl Native {
    /// A watcher whose notices go into `signals`; a full channel sets `overflow`.
    pub(crate) fn new(
        signals: mpsc::Sender<Signal>,
        overflow: Arc<AtomicBool>,
    ) -> notify::Result<Self> {
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let Some(signal) = signal_of(result)
                && signals.try_send(signal).is_err()
            {
                overflow.store(true, Ordering::Release);
            }
        })?;
        Ok(Self {
            watcher,
            watched: BTreeSet::new(),
        })
    }

    /// Watches exactly the folders `wanted`; the ones the system refuses are returned, for
    /// the fallback (`poll.rs`).
    pub(crate) fn set(&mut self, wanted: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
        let gone: Vec<PathBuf> = self.watched.difference(wanted).cloned().collect();
        for path in gone {
            // A folder deleted meanwhile is unwatched already.
            let _ = self.watcher.unwatch(&path);
            self.watched.remove(&path);
        }
        let mut refused = BTreeSet::new();
        let new: Vec<PathBuf> = wanted.difference(&self.watched).cloned().collect();
        for path in new {
            match self.watcher.watch(&path, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    self.watched.insert(path);
                }
                Err(error) => {
                    tracing::debug!(%error, "a folder cannot be watched; its time is polled");
                    refused.insert(path);
                }
            }
        }
        refused
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
