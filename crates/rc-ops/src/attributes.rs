//! Attributes and times for a selection, optionally down its folders (RC-020-06, Total
//! Commander's "Change attributes" with "Process subdirectories").
//!
//! The chosen entries always change; below them, `apply_to` picks files, folders or both, so
//! a mode meant for files does not lock the folders. Links below the start are never entered
//! and get no mode or read-only flag - set through a link, those land on its target, which may
//! lie outside the tree; their times change as such (their target's on Windows). A failure below the start is reported and the rest goes
//! on; the start failing fails the whole, before anything below it is touched.

use std::path::{Path, PathBuf};

use rc_vfs::{EntryKind, PropertyChange, Vfs, VfsError};
use serde::{Deserialize, Serialize};

/// Failures listed at most in a report; the count goes on.
pub const MAX_LISTED_FAILURES: usize = 100;

/// Which entries below a chosen folder change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyTo {
    pub files: bool,
    pub dirs: bool,
}

impl Default for ApplyTo {
    fn default() -> Self {
        Self {
            files: true,
            dirs: true,
        }
    }
}

/// One entry that could not be changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeFailure {
    pub path: PathBuf,
    /// The error code (`permission_denied`, `unsupported`, ...).
    pub code: String,
}

/// What a change did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeReport {
    /// Entries changed.
    pub changed: u64,
    /// Entries that failed, [`MAX_LISTED_FAILURES`] at most.
    pub failures: Vec<AttributeFailure>,
    /// All entries that failed.
    pub failed: u64,
}

impl AttributeReport {
    /// Counts a failure on `path`, and lists it while there is room.
    pub fn record(&mut self, path: &Path, error: &VfsError) {
        self.failed += 1;
        if self.failures.len() < MAX_LISTED_FAILURES {
            self.failures.push(AttributeFailure {
                path: path.to_path_buf(),
                code: error.code().to_owned(),
            });
        }
    }
}

/// Applies `change` to `path` and, with `recursive`, to what `apply_to` names below it.
///
/// # Errors
///
/// `path` itself cannot be changed; nothing below it was touched then.
pub async fn set_properties_tree(
    vfs: &dyn Vfs,
    path: &Path,
    change: PropertyChange,
    recursive: bool,
    apply_to: ApplyTo,
    report: &mut AttributeReport,
) -> Result<(), VfsError> {
    let entry = vfs.stat(path).await?;
    vfs.set_properties(path, change).await?;
    report.changed += 1;
    if !recursive || entry.kind != EntryKind::Dir {
        return Ok(());
    }
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let listed = match vfs.list(&dir).await {
            Ok(listed) => listed,
            Err(error) => {
                report.record(&dir, &error);
                continue;
            }
        };
        for entry in listed {
            let child = dir.join(&entry.name);
            let is_dir = entry.kind == EntryKind::Dir;
            if is_dir {
                pending.push(child.clone());
            }
            if (is_dir && !apply_to.dirs) || (!is_dir && !apply_to.files) {
                continue;
            }
            let change = if matches!(entry.kind, EntryKind::Symlink { .. }) {
                // A link below the start may lead anywhere - `key -> ~/.ssh/id_rsa` - and a mode
                // or read-only flag set through it lands on its target, outside the tree.
                link_change(change)
            } else {
                change
            };
            if change.is_empty() {
                continue;
            }
            match vfs.set_properties(&child, change).await {
                Ok(()) => report.changed += 1,
                Err(error) => report.record(&child, &error),
            }
        }
    }
    Ok(())
}

/// What of `change` a link below the start gets: its own times and Windows flags, never a mode
/// or the read-only flag, which the file systems set on the target.
fn link_change(change: PropertyChange) -> PropertyChange {
    PropertyChange {
        mode: None,
        readonly: None,
        ..change
    }
}

#[cfg(test)]
#[path = "attributes_tests.rs"]
mod tests;
