//! The walk: one directory listed at a time, depth first, events pulled one by one.
//!
//! Pulling ([`Walk::next`]) keeps the walk a plain value: the streaming search pumps it into a
//! channel, the paged command stops pulling once its page is full. Memory holds the directory
//! being searched and the folders still to enter, never the hits.

use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use rc_vfs::{BackendId, Entry, EntryKind, Vfs, VfsError, VfsPath};

use super::{
    Compiled, SearchCancel, SearchEvent, SearchHit, SearchStats,
    content::{ContentMatcher, Scan},
    filters::shape,
};
use crate::read_guard::ReadGuard;

/// How often a [`SearchEvent::Progress`] is reported at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// Links followed in a row at most: the guard for cycles a path comparison cannot see (a link
/// whose target runs through another link).
const MAX_LINKS: u32 = 32;

/// A directory waiting to be searched.
#[derive(Debug)]
struct Dir {
    /// As the walk reached it, which is how hits below it are reported.
    path: PathBuf,
    /// With the links on the way replaced by their targets (lexically): what the cycle guard
    /// compares.
    real: PathBuf,
    depth: u32,
    links: u32,
}

/// The directory being searched.
struct Listing {
    dir: Dir,
    entries: std::vec::IntoIter<Entry>,
    /// Its subdirectories to enter once its own entries are done.
    subdirs: Vec<Dir>,
}

/// A running search.
pub(crate) struct Walk {
    vfs: Arc<dyn Vfs>,
    backend: BackendId,
    query: Compiled,
    cancel: SearchCancel,
    /// The read guard of an MCP search: the files whose contents it never reads.
    guard: Option<ReadGuard>,
    stack: Vec<Dir>,
    current: Option<Listing>,
    /// Real directories entered through a link: each is searched once.
    visited: HashSet<PathBuf>,
    stats: SearchStats,
    buf: Vec<u8>,
    now: SystemTime,
    last_progress: Instant,
    finished: bool,
}

impl Walk {
    /// A search of `roots` (paths of `vfs`, all on `backend`).
    pub(crate) fn new(
        vfs: Arc<dyn Vfs>,
        backend: BackendId,
        roots: Vec<PathBuf>,
        query: Compiled,
        cancel: SearchCancel,
    ) -> Self {
        let stack = roots
            .into_iter()
            .rev()
            .map(|path| Dir {
                real: fold(&path),
                path,
                depth: 0,
                links: 0,
            })
            .collect();
        Self {
            vfs,
            backend,
            query,
            cancel,
            guard: None,
            stack,
            current: None,
            visited: HashSet::new(),
            stats: SearchStats::default(),
            buf: Vec::new(),
            now: SystemTime::now(),
            last_progress: Instant::now(),
            finished: false,
        }
    }

    /// The walk with the read guard of an MCP search, if any.
    pub(crate) fn guarded(mut self, guard: Option<ReadGuard>) -> Self {
        self.guard = guard;
        self
    }

    /// The counts so far.
    pub(crate) fn stats(&self) -> SearchStats {
        self.stats
    }

    /// The next event; `None` after [`SearchEvent::Finished`].
    pub(crate) async fn next(&mut self) -> Option<SearchEvent> {
        loop {
            if self.finished {
                return None;
            }
            if self.cancel.is_cancelled() {
                return Some(self.finish(true));
            }
            if self.last_progress.elapsed() >= PROGRESS_EVERY {
                self.last_progress = Instant::now();
                return Some(SearchEvent::Progress {
                    dir: self
                        .current
                        .as_ref()
                        .map(|listing| self.at(&listing.dir.path)),
                    stats: self.stats,
                });
            }
            if let Some(entry) = self.current.as_mut().and_then(|l| l.entries.next()) {
                if let Some(event) = self.visit(entry).await {
                    return Some(event);
                }
                continue;
            }
            if let Some(done) = self.current.take() {
                self.stack.extend(done.subdirs.into_iter().rev());
            }
            let Some(dir) = self.stack.pop() else {
                return Some(self.finish(false));
            };
            match self.vfs.list(&dir.path).await {
                Ok(mut entries) => {
                    entries.sort_by(|a, b| a.name.cmp(&b.name));
                    self.stats.dirs += 1;
                    self.current = Some(Listing {
                        dir,
                        entries: entries.into_iter(),
                        subdirs: Vec::new(),
                    });
                }
                Err(error) => {
                    self.stats.skipped += 1;
                    return Some(SearchEvent::Skipped {
                        path: self.at(&dir.path),
                        code: error.code(),
                    });
                }
            }
        }
    }

    fn finish(&mut self, cancelled: bool) -> SearchEvent {
        self.finished = true;
        self.current = None;
        self.stack.clear();
        SearchEvent::Finished {
            stats: self.stats,
            cancelled,
        }
    }

    fn at(&self, path: &Path) -> VfsPath {
        VfsPath {
            backend: self.backend.clone(),
            path: path.to_path_buf(),
        }
    }

    /// One entry of the current directory: queued to enter when it is a directory, reported
    /// when it matches.
    async fn visit(&mut self, entry: Entry) -> Option<SearchEvent> {
        self.stats.entries += 1;
        if self.query.skip_hidden && entry.hidden {
            return None;
        }
        let listing = self.current.as_mut()?;
        let path = listing.dir.path.join(&entry.name);
        let (file, dir) = shape(&entry);
        if dir && let Some(sub) = descend(&self.query, &mut self.visited, &listing.dir, &entry) {
            listing.subdirs.push(sub);
        }
        if !self.query.names.matches(&entry.name, dir)
            || !self.query.filters.admits(&entry, self.now)
        {
            return None;
        }
        if let Some(content) = &self.query.content {
            let link = matches!(entry.kind, EntryKind::Symlink { .. });
            if !file || (link && !self.query.follow_symlinks) {
                return None;
            }
            // A guarded file is never read, so neither its presence among the hits nor its
            // absence tells about its contents: it only counts as skipped.
            if let Some(guard) = self.guard
                && guard.refuses_off_thread(&path).await.is_some()
            {
                self.stats.skipped += 1;
                return Some(SearchEvent::Skipped {
                    path: self.at(&path),
                    code: "sensitive_path",
                });
            }
            let scan = scan_file(&*self.vfs, &path, content, &mut self.buf, &self.cancel).await;
            match scan {
                Ok(Scan::Found) if !content.not_containing => {}
                Ok(Scan::Absent) if content.not_containing => {}
                Ok(_) => return None,
                Err(error) => {
                    self.stats.skipped += 1;
                    return Some(SearchEvent::Skipped {
                        path: self.at(&path),
                        code: error.code(),
                    });
                }
            }
        }
        self.stats.hits += 1;
        Some(SearchEvent::Hit(SearchHit {
            path: self.at(&path),
            entry,
        }))
    }
}

/// The subdirectory `entry` of `parent` to enter, if the depth limit, the directory
/// exclusions and - for a link - the cycle guard allow it.
fn descend(
    query: &Compiled,
    visited: &mut HashSet<PathBuf>,
    parent: &Dir,
    entry: &Entry,
) -> Option<Dir> {
    let depth = parent.depth + 1;
    if query.max_depth.is_some_and(|max| depth > max) || !query.names.enters(&entry.name) {
        return None;
    }
    let path = parent.path.join(&entry.name);
    match &entry.kind {
        EntryKind::Dir => Some(Dir {
            path,
            real: parent.real.join(&entry.name),
            depth,
            links: parent.links,
        }),
        EntryKind::Symlink { target, .. } if query.follow_symlinks => {
            if parent.links >= MAX_LINKS {
                return None;
            }
            let real = fold(&parent.real.join(target));
            // A link to its own directory or one above it would loop; a directory reached
            // through another link already is searched once.
            if parent.real.starts_with(&real) || !visited.insert(real.clone()) {
                return None;
            }
            Some(Dir {
                path,
                real,
                depth,
                links: parent.links + 1,
            })
        }
        _ => None,
    }
}

/// Reads one file through `matcher`.
async fn scan_file(
    vfs: &dyn Vfs,
    path: &Path,
    matcher: &ContentMatcher,
    buf: &mut Vec<u8>,
    cancel: &SearchCancel,
) -> Result<Scan, VfsError> {
    let mut stream = vfs.open_read(path).await?;
    matcher
        .scan(&mut stream, buf, cancel)
        .await
        .map_err(|error| VfsError::from_io(error, path))
}

/// `.` and `..` folded away without asking the file system (`..` at the root stays there).
fn fold(path: &Path) -> PathBuf {
    let mut folded = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(folded.components().next_back(), Some(Component::Normal(_))) {
                    folded.pop();
                }
            }
            other => folded.push(other),
        }
    }
    folded
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
