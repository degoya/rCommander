//! What stays of the copies kept for taking an update back (RC-100-07).
//!
//! Once the last update is proven - its journal says `verified` for the version running now and no
//! updater runs (`recover::update_proven`) - the older database copies in `<data>/update/pre-update/`
//! take nothing back any more: a roll-back reaches for the newest one. [`sweep`] keeps exactly that
//! one, and after `update.backup_retention_days` that one goes too (0 keeps it for good); the
//! verified downloads in `<data>/update/download/` go, and `<install>/.trash/` is swept. An update
//! that is not proven - waiting for its proof, rolled back, failed - keeps everything.
//!
//! Only files this crate names are ever touched: database copies `rcommander-*.db`, downloads
//! `rcommander-*.zip` and `rcommander-*.tar.gz`. Foreign files and `.partial` downloads stay.
//!
//! Adapted from rDownloader's `crates/rd-backup/src/update_retention.rs` at commit 26c8592a6:
//! reduced to the two folders, no plugin cache.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::install::{DOWNLOAD_DIR, PRE_UPDATE_DIR, trash, update_dir};

/// What a sweep may do.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// The last update is proven, or none is recorded.
    pub proven: bool,
    /// Days the newest copy stays after a proven update; 0 keeps it for good.
    pub retention_days: u32,
    pub now: SystemTime,
}

/// What a sweep removed, and what stays.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub removed: Vec<PathBuf>,
    pub freed_bytes: u64,
}

/// A database copy this crate writes: `rcommander-<version>-<time>.db`.
#[must_use]
pub fn is_database_copy(name: &str) -> bool {
    name.starts_with("rcommander-") && name.ends_with(".db")
}

/// A verified download this crate keeps: the package's archive, never a `.partial`.
#[must_use]
pub fn is_download(name: &str) -> bool {
    name.starts_with("rcommander-") && (name.ends_with(".zip") || name.ends_with(".tar.gz"))
}

/// Applies `policy` to the folders of `data` and sweeps the trash of `install`.
#[must_use]
pub fn sweep(data: &Path, install: &Path, policy: Policy) -> Report {
    let mut report = Report::default();
    if !policy.proven {
        return report;
    }
    let update = update_dir(data);
    let mut copies = files(&update.join(PRE_UPDATE_DIR), is_database_copy);
    // Newest first; the newest stays until its time is up.
    copies.sort_by_key(|copy| std::cmp::Reverse(copy.1));
    let grace = Duration::from_secs(u64::from(policy.retention_days) * 24 * 60 * 60);
    for (index, (path, modified, size)) in copies.into_iter().enumerate() {
        let expired = policy.retention_days > 0
            && modified.is_some_and(|modified| {
                policy
                    .now
                    .duration_since(modified)
                    .is_ok_and(|age| age >= grace)
            });
        if index == 0 && !expired {
            continue;
        }
        remove(&mut report, path, size);
    }
    for (path, _, size) in files(&update.join(DOWNLOAD_DIR), is_download) {
        remove(&mut report, path, size);
    }
    trash::sweep(install);
    report
}

/// The bytes the leftovers of updates take now: copies, downloads and the trash.
#[must_use]
pub fn leftover_bytes(data: &Path, install: &Path) -> u64 {
    let update = update_dir(data);
    let named = files(&update.join(PRE_UPDATE_DIR), is_database_copy)
        .into_iter()
        .chain(files(&update.join(DOWNLOAD_DIR), is_download))
        .map(|(_, _, size)| size)
        .sum::<u64>();
    named.saturating_add(tree_bytes(&trash::trash_dir(install)))
}

fn remove(report: &mut Report, path: PathBuf, size: u64) {
    match fs::remove_file(&path) {
        Ok(()) => {
            report.freed_bytes = report.freed_bytes.saturating_add(size);
            report.removed.push(path);
        }
        Err(error) => tracing::warn!(%error, "an update copy stays until a later sweep"),
    }
}

/// The plain files of `folder` whose names `keep` accepts: path, modified time, size.
fn files(folder: &Path, keep: fn(&str) -> bool) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(keep))
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .is_file()
                .then(|| (entry.path(), metadata.modified().ok(), metadata.len()))
        })
        .collect()
}

fn tree_bytes(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| tree_bytes(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;
