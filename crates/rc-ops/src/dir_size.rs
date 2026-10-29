//! Occupied space (RC-020-06): the bytes, files and folders under a path, as Total Commander's
//! Space on a folder, Alt+Shift+Enter and Ctrl+L count them.
//!
//! The walk lists each folder once ([`Vfs::list_details`]), never follows a link, and counts a
//! file with several names (a hard link) once where the backend tells identities. A folder that
//! cannot be read is counted as unreadable and left out; only the start failing fails the count.
//! There is no cancel switch: dropping the future stops the walk between two folders.

use std::{collections::HashSet, path::Path};

use rc_vfs::{EntryKind, FileId, Vfs, VfsError};
use serde::{Deserialize, Serialize};

/// What a count found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirUsage {
    /// Bytes of the files (a link counts nothing, a hard-linked file once).
    pub bytes: u64,
    /// Bytes the files occupy on the volume, when the backend told it for every one.
    pub allocated: Option<u64>,
    /// Files, links and other entries that are no folder.
    pub files: u64,
    /// Folders below the start (the start not counted).
    pub dirs: u64,
    /// Names of a file met before under another name, not counted again.
    pub hard_links: u64,
    /// Folders that could not be listed.
    pub unreadable: u64,
}

impl DirUsage {
    /// Both counts together, as a sum over several starts.
    pub fn add(&mut self, other: &Self) {
        self.bytes += other.bytes;
        self.allocated = self.allocated.zip(other.allocated).map(|(a, b)| a + b);
        self.files += other.files;
        self.dirs += other.dirs;
        self.hard_links += other.hard_links;
        self.unreadable += other.unreadable;
    }
}

/// Counts what is at `path`: a folder with everything below it, or a single entry.
///
/// # Errors
///
/// `path` cannot be read.
pub async fn dir_size(vfs: &dyn Vfs, path: &Path) -> Result<DirUsage, VfsError> {
    let mut seen = HashSet::new();
    dir_size_seen(vfs, path, &mut seen).await
}

/// [`dir_size`] for several starts, a hard link shared between them counted once; one count per
/// start in order, and the sum.
///
/// # Errors
///
/// The first start that cannot be read.
pub async fn dir_sizes(
    vfs: &dyn Vfs,
    paths: &[&Path],
) -> Result<(Vec<DirUsage>, DirUsage), VfsError> {
    let mut seen = HashSet::new();
    let mut total = DirUsage {
        allocated: Some(0),
        ..DirUsage::default()
    };
    let mut each = Vec::with_capacity(paths.len());
    for path in paths {
        let usage = dir_size_seen(vfs, path, &mut seen).await?;
        total.add(&usage);
        each.push(usage);
    }
    Ok((each, total))
}

async fn dir_size_seen(
    vfs: &dyn Vfs,
    path: &Path,
    seen: &mut HashSet<FileId>,
) -> Result<DirUsage, VfsError> {
    let entry = vfs.stat(path).await?;
    let mut usage = DirUsage {
        allocated: Some(0),
        ..DirUsage::default()
    };
    if entry.kind != EntryKind::Dir {
        let details = vfs.details(path).await?;
        count_file(&mut usage, &entry.kind, entry.size, &details, seen);
        return Ok(usage);
    }
    let mut pending = vec![path.to_path_buf()];
    let mut first = true;
    while let Some(dir) = pending.pop() {
        let listed = match vfs.list_details(&dir).await {
            Ok(listed) => listed,
            Err(error) if first => return Err(error),
            Err(_) => {
                usage.unreadable += 1;
                continue;
            }
        };
        first = false;
        for (entry, details) in listed {
            if entry.kind == EntryKind::Dir {
                usage.dirs += 1;
                pending.push(dir.join(&entry.name));
            } else {
                count_file(&mut usage, &entry.kind, entry.size, &details, seen);
            }
        }
    }
    Ok(usage)
}

/// One entry that is no folder: a link or a special file adds no bytes, a file seen before
/// under another name adds nothing but to `hard_links`.
fn count_file(
    usage: &mut DirUsage,
    kind: &EntryKind,
    size: u64,
    details: &rc_vfs::Details,
    seen: &mut HashSet<FileId>,
) {
    if let Some(id) = details.id
        && !seen.insert(id)
    {
        usage.hard_links += 1;
        return;
    }
    usage.files += 1;
    if *kind == EntryKind::File {
        usage.bytes += size;
    }
    usage.allocated = usage
        .allocated
        .zip(details.allocated)
        .map(|(sum, more)| sum + more);
}

#[cfg(test)]
#[path = "dir_size_tests.rs"]
mod tests;
