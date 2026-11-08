//! The disk-usage analysis (RC-100-98, rCommander's own, as ncdu and TreeSize show it): one walk
//! over any backend that keeps the sums of every folder below the start - bytes, occupied bytes,
//! files, folders - and the largest files met.
//!
//! The walk is the size count's (`rc_ops::walk_usage`): each folder listed once, links never
//! followed, a hard-linked file counted once, a folder that cannot be read counted as unreadable.
//! Its visitor ([`Tally`]) adds every file to its folder and all folders above it as it comes, so
//! the tree holds true sums of what was read so far at any moment: the window reads it while the
//! walk goes on ([`Engine::start_disk_usage`], [`DiskUsageHandle`]), `files.disk_usage` once the
//! walk is through. The tree keeps at most [`MAX_FOLDERS`] folders and the largest files a heap
//! of their number; past the folders it stops and says it was cut. A stop keeps what was found.

mod tree;

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use rc_ops::{DirUsage, walk_usage};
use rc_vfs::{Vfs, VfsPath};
use serde::{Deserialize, Serialize};

use self::tree::Tally;
pub use self::tree::{FolderRow, LargeFile, SizeBy, UsageNode, UsageTree};
use crate::recover::LockOrRecover as _;
use crate::{
    Engine, EngineError, Outcome, PanelId,
    search::SearchCancel,
    settings::{DISK_USAGE_LARGEST_KEY, DISK_USAGE_SIZE_KEY},
};

/// Folders the tree keeps at most; a safety limit, not a setting.
pub const MAX_FOLDERS: usize = 1_000_000;
/// How often the window reads the tree while the walk goes on.
pub const REFRESH_EVERY: Duration = Duration::from_millis(200);
/// The levels of subfolders `files.disk_usage` reports at most.
pub const MAX_DEPTH: u32 = 3;
/// Folders per level and largest files `files.disk_usage` reports when the call names no limit.
pub const DEFAULT_LIMIT: usize = 20;
/// ... and at most.
pub const MAX_LIMIT: usize = 1000;

/// What `files.disk_usage` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskUsageReport {
    /// The folder analysed.
    pub path: VfsPath,
    /// Everything below it.
    pub usage: DirUsage,
    /// Its subfolders by size, largest first, each with its own to the depth asked.
    pub folders: Vec<FolderUsage>,
    /// Its subfolders left out past the limit.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more: usize,
    /// The largest files below it, largest first, as paths relative to it.
    pub largest: Vec<LargeFile>,
    /// The walk stopped at [`MAX_FOLDERS`]: the sums count only what it read.
    pub truncated: bool,
}

/// A subfolder in a [`DiskUsageReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderUsage {
    pub name: String,
    pub usage: DirUsage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<FolderUsage>,
    /// Its subfolders left out past the limit.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more: usize,
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

/// What the window shows and keeps, from the settings `disk_usage.size` and
/// `disk_usage.largest_files`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageChoices {
    pub size_by: SizeBy,
    pub largest: usize,
}

impl Default for UsageChoices {
    fn default() -> Self {
        Self {
            size_by: SizeBy::Bytes,
            largest: 100,
        }
    }
}

/// A disk-usage walk running in the background: its tree, read at any time, and its stop.
/// Clones share both; dropping one stops nothing - the window stops the walk when it closes.
#[derive(Debug, Clone)]
pub struct DiskUsageHandle {
    tree: Arc<Mutex<UsageTree>>,
    cancel: SearchCancel,
}

impl DiskUsageHandle {
    /// Stops the walk at its next entry; what it found stays.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Reads the tree as it stands.
    pub fn read<T>(&self, look: impl FnOnce(&UsageTree) -> T) -> T {
        look(&self.tree.lock_or_recover("disk_usage.tree"))
    }

    /// Whether the walk ended: through, stopped or failed.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.read(UsageTree::finished)
    }
}

impl Engine {
    /// Starts the analysis of `path` (relative to the panel's folder, the folder itself when
    /// `None`) in the background, keeping the `largest` largest files.
    ///
    /// # Errors
    ///
    /// The folder cannot be reached (an unopened server).
    pub fn start_disk_usage(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
        largest: usize,
    ) -> Result<DiskUsageHandle, EngineError> {
        let root = self.usage_root(panel, path);
        let vfs = self.vfs(&root)?;
        let tree = Arc::new(Mutex::new(UsageTree::new(
            root.clone(),
            largest,
            MAX_FOLDERS,
        )));
        let cancel = SearchCancel::default();
        let handle = DiskUsageHandle {
            tree: Arc::clone(&tree),
            cancel: cancel.clone(),
        };
        tokio::spawn(async move {
            run(&*vfs, &root.path, tree, cancel).await;
        });
        Ok(handle)
    }

    /// `files.disk_usage`: the analysis of `path` run to its end in the call - cancelling the
    /// call stops it -, reported to `depth` levels of subfolders with `limit` folders per level
    /// and `limit` largest files.
    pub(crate) async fn disk_usage(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
        depth: Option<u32>,
        limit: Option<usize>,
    ) -> Result<Outcome, EngineError> {
        let root = self.usage_root(panel, path);
        let vfs = self.vfs(&root)?;
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let depth = depth.unwrap_or(1).clamp(1, MAX_DEPTH);
        let tree = Arc::new(Mutex::new(UsageTree::new(root.clone(), limit, MAX_FOLDERS)));
        let cancel = SearchCancel::default();
        if let Some(error) = run(&*vfs, &root.path, Arc::clone(&tree), cancel).await {
            return Err(error.into());
        }
        let tree = tree.lock_or_recover("disk_usage.tree");
        Ok(Outcome::DiskUsage(Box::new(tree.report(depth, limit))))
    }

    /// The settings of the disk-usage window: stored, or their defaults.
    pub async fn disk_usage_choices(&self) -> UsageChoices {
        let read = |key: &'static str| async move {
            match self.db() {
                Ok(db) => crate::settings::stored_setting(db, key).await,
                Err(_) => crate::settings::setting(key).map(|spec| spec.default_value()),
            }
        };
        let size_by = read(DISK_USAGE_SIZE_KEY).await;
        let largest = read(DISK_USAGE_LARGEST_KEY).await;
        let defaults = UsageChoices::default();
        UsageChoices {
            size_by: size_by
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .map_or(defaults.size_by, SizeBy::from_setting),
            largest: largest
                .as_ref()
                .and_then(serde_json::Value::as_u64)
                .and_then(|largest| usize::try_from(largest).ok())
                .unwrap_or(defaults.largest),
        }
    }

    /// `files.disk_usage_dialog`: announces that the disk-usage window is wanted for the panel.
    pub(crate) fn disk_usage_dialog(&self, panel: Option<PanelId>) -> Outcome {
        let id = self.resolve(panel);
        self.publish(crate::EngineEvent::DiskUsage { panel: id });
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// The folder an analysis starts in.
    fn usage_root(&self, panel: Option<PanelId>, path: Option<&str>) -> VfsPath {
        let id = self.resolve(panel);
        let current = self.state().panel(id).path.clone();
        match path {
            Some(path) => crate::commands::resolve_path(&current, path),
            None => current,
        }
    }
}

/// Walks `root` into `tree` until the end or `cancel`; the walk's error when the start could not
/// be read.
pub(crate) async fn run(
    vfs: &dyn Vfs,
    root: &Path,
    tree: Arc<Mutex<UsageTree>>,
    cancel: SearchCancel,
) -> Option<rc_vfs::VfsError> {
    let mut tally = Tally::new(Arc::clone(&tree), cancel.clone());
    let walked = walk_usage(vfs, root, UsageTree::ROOT, &mut tally).await;
    let mut tree = tree.lock_or_recover("disk_usage.tree");
    match walked {
        Ok(walked) => {
            tree.finish(walked.stopped && cancel.is_cancelled());
            None
        }
        Err(error) => {
            tree.fail(error.code());
            Some(error)
        }
    }
}

#[cfg(test)]
#[path = "disk_usage_tests.rs"]
mod tests;
