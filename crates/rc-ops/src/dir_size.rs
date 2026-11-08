//! Occupied space (RC-020-06): the bytes, files and folders under a path, as Total Commander's
//! Space on a folder, Alt+Shift+Enter and Ctrl+L count them.
//!
//! The walk lists each folder once ([`Vfs::list_details`]), never follows a link, and counts a
//! file with several names (a hard link) once where the backend tells identities. A folder that
//! cannot be read is counted as unreadable and left out; only the start failing fails the count.
//! There is no cancel switch: dropping the future stops the walk between two folders.
//!
//! The same walk serves the disk-usage analysis (RC-100-98): [`walk_usage`] shows a
//! [`UsageVisitor`] every folder and file it counts, and the visitor may stop it; [`dir_size`] is
//! that walk with a visitor that only lets it run.

use std::{
    collections::HashSet,
    ops::ControlFlow,
    path::{Path, PathBuf},
};

use rc_vfs::{Entry, EntryKind, FileId, Vfs, VfsError};

use crate::SkipEntry;
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

/// What one counted entry that is no folder adds: its bytes (none for a link or a special file)
/// and what it occupies on the volume, when the backend tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counted {
    pub bytes: u64,
    pub allocated: Option<u64>,
}

/// What [`walk_usage`] shows as it counts. Each folder has a handle of the visitor's choosing,
/// handed back with every entry of that folder; the start's is given to the walk.
pub trait UsageVisitor {
    /// The visitor's handle of a folder.
    type Folder: Send;

    /// The folder `entry` at `path`, met in the listing of `parent`: its handle, or `Break` to
    /// stop the walk.
    fn folder(
        &mut self,
        parent: &Self::Folder,
        entry: &Entry,
        path: &Path,
    ) -> ControlFlow<(), Self::Folder>;

    /// An entry of `parent` that is no folder: `Some` when it was counted, `None` for another
    /// name of a file counted before (a hard link). `Break` stops the walk.
    fn file(
        &mut self,
        parent: &Self::Folder,
        entry: &Entry,
        counted: Option<Counted>,
    ) -> ControlFlow<()>;

    /// The folder `folder` could not be listed; the walk goes on without it.
    fn unreadable(&mut self, _folder: &Self::Folder) {}
}

/// How a [`walk_usage`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageWalk {
    /// What was counted, up to the stop when there was one.
    pub usage: DirUsage,
    /// The visitor stopped the walk before its end.
    pub stopped: bool,
}

/// Counts what is at `path`: a folder with everything below it, or a single entry.
///
/// # Errors
///
/// `path` cannot be read.
pub async fn dir_size(vfs: &dyn Vfs, path: &Path) -> Result<DirUsage, VfsError> {
    dir_size_without(vfs, path, &|_, _| false).await
}

/// [`dir_size`] leaving out every entry below `path` that `skip` hides, listed in its folder:
/// the panels' ignore list (RC-100-92). A folder left out is not entered.
///
/// # Errors
///
/// `path` cannot be read.
pub async fn dir_size_without(
    vfs: &dyn Vfs,
    path: &Path,
    skip: SkipEntry<'_>,
) -> Result<DirUsage, VfsError> {
    let mut seen = HashSet::new();
    let walked = walk_seen(vfs, path, (), (&mut JustCount, skip), &mut seen).await?;
    Ok(walked.usage)
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
        let usage = walk_seen(vfs, path, (), (&mut JustCount, &|_, _| false), &mut seen)
            .await?
            .usage;
        total.add(&usage);
        each.push(usage);
    }
    Ok((each, total))
}

/// [`dir_size`] with `visitor` shown every folder and file as they are counted; `root` is the
/// start's handle. The visitor's `Break` ends the walk there, the count so far in the answer.
///
/// # Errors
///
/// `path` cannot be read.
pub async fn walk_usage<V: UsageVisitor + Send>(
    vfs: &dyn Vfs,
    path: &Path,
    root: V::Folder,
    visitor: &mut V,
) -> Result<UsageWalk, VfsError> {
    let mut seen = HashSet::new();
    walk_seen(vfs, path, root, (visitor, &|_, _| false), &mut seen).await
}

/// The visitor of a plain count: it lets the walk run.
struct JustCount;

impl UsageVisitor for JustCount {
    type Folder = ();

    fn folder(&mut self, (): &(), _: &Entry, _: &Path) -> ControlFlow<(), ()> {
        ControlFlow::Continue(())
    }

    fn file(&mut self, (): &(), _: &Entry, _: Option<Counted>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }
}

async fn walk_seen<V: UsageVisitor + Send>(
    vfs: &dyn Vfs,
    path: &Path,
    root: V::Folder,
    (visitor, skip): (&mut V, SkipEntry<'_>),
    seen: &mut HashSet<FileId>,
) -> Result<UsageWalk, VfsError> {
    let entry = vfs.stat(path).await?;
    let mut usage = DirUsage {
        allocated: Some(0),
        ..DirUsage::default()
    };
    if entry.kind != EntryKind::Dir {
        let details = vfs.details(path).await?;
        let counted = count_file(&mut usage, &entry.kind, entry.size, &details, seen);
        let stopped = visitor.file(&root, &entry, counted).is_break();
        return Ok(UsageWalk { usage, stopped });
    }
    let mut pending: Vec<(PathBuf, V::Folder)> = vec![(path.to_path_buf(), root)];
    let mut first = true;
    while let Some((dir, folder)) = pending.pop() {
        let listed = match vfs.list_details(&dir).await {
            Ok(listed) => listed,
            Err(error) if first => return Err(error),
            Err(_) => {
                usage.unreadable += 1;
                visitor.unreadable(&folder);
                continue;
            }
        };
        first = false;
        for (entry, details) in listed {
            if skip(&dir, &entry) {
                continue;
            }
            let flow = if entry.kind == EntryKind::Dir {
                usage.dirs += 1;
                let path = dir.join(&entry.name);
                match visitor.folder(&folder, &entry, &path) {
                    ControlFlow::Continue(below) => {
                        pending.push((path, below));
                        ControlFlow::Continue(())
                    }
                    ControlFlow::Break(()) => ControlFlow::Break(()),
                }
            } else {
                let counted = count_file(&mut usage, &entry.kind, entry.size, &details, seen);
                visitor.file(&folder, &entry, counted)
            };
            if flow.is_break() {
                return Ok(UsageWalk {
                    usage,
                    stopped: true,
                });
            }
        }
    }
    Ok(UsageWalk {
        usage,
        stopped: false,
    })
}

/// One entry that is no folder: a link or a special file adds no bytes, a file seen before
/// under another name adds nothing but to `hard_links` (`None`).
fn count_file(
    usage: &mut DirUsage,
    kind: &EntryKind,
    size: u64,
    details: &rc_vfs::Details,
    seen: &mut HashSet<FileId>,
) -> Option<Counted> {
    if let Some(id) = details.id
        && !seen.insert(id)
    {
        usage.hard_links += 1;
        return None;
    }
    usage.files += 1;
    let bytes = if *kind == EntryKind::File { size } else { 0 };
    usage.bytes += bytes;
    usage.allocated = usage
        .allocated
        .zip(details.allocated)
        .map(|(sum, more)| sum + more);
    Some(Counted {
        bytes,
        allocated: details.allocated,
    })
}

#[cfg(test)]
#[path = "dir_size_tests.rs"]
mod tests;
