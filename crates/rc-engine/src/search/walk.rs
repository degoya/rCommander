//! The walk: one directory listed at a time, depth first, events pulled one by one.
//!
//! Pulling ([`Walk::next`]) keeps the walk a plain value: the streaming search pumps it into a
//! channel, the paged command stops pulling once its page is full. Memory holds the directory
//! being searched and the folders still to enter, never the hits.
//!
//! An archive file, when archives are searched, waits among the folders of its directory and is
//! opened when its turn comes ([`super::archive`]); its folders carry the archive along and drop
//! it with the last one. Fed Everything's candidates instead of start folders, the walk lists
//! nothing and judges each candidate as it would have met it ([`candidates`]).

mod candidates;

use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use rc_vfs::{ArchiveLimits, BackendId, Entry, EntryKind, Vfs, VfsError, VfsPath};

use self::candidates::Candidates;
use super::{
    Compiled, SearchCancel, SearchEvent, SearchHit, SearchStats,
    archive::{self, Scope},
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
    /// The archive it lies in; `None` on the walk's own backend.
    scope: Option<Arc<Scope>>,
    /// An archive file to open and search from its root, not a directory to list.
    archive: bool,
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
    /// The limits archives open with; `None` when no archive is entered.
    limits: Option<ArchiveLimits>,
    /// Everything's candidates, when they stand in for the folders.
    candidates: Option<Candidates>,
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
                scope: None,
                archive: false,
            })
            .collect();
        let limits =
            (query.archives && archive::searched_on(&backend)).then(ArchiveLimits::default);
        Self {
            vfs,
            backend,
            query,
            cancel,
            guard: None,
            limits,
            candidates: None,
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

    /// The walk opening archives within `limits` (the engine's decoder memory), when it enters
    /// any.
    pub(crate) fn with_archive_limits(mut self, limits: ArchiveLimits) -> Self {
        self.limits = self.limits.map(|_| limits);
        self
    }

    /// Whether the walk enters archives.
    pub(crate) fn enters_archives(&self) -> bool {
        self.limits.is_some()
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
                        .map(|listing| self.at(&listing.dir, &listing.dir.path)),
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
            if self.candidates.is_some() {
                match self.next_candidate().await {
                    Some(Some(event)) => return Some(event),
                    Some(None) => continue,
                    None => return Some(self.finish(false)),
                }
            }
            let Some(mut dir) = self.stack.pop() else {
                return Some(self.finish(false));
            };
            if dir.archive {
                match self.enter(&dir).await {
                    Ok(Some(root)) => dir = root,
                    Ok(None) => continue,
                    Err((path, code)) => return Some(SearchEvent::Skipped { path, code }),
                }
            }
            let vfs = Arc::clone(dir.scope.as_ref().map_or(&self.vfs, |scope| &scope.vfs));
            match vfs.list(&dir.path).await {
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
                        path: self.at(&dir, &dir.path),
                        code: error.code(),
                    });
                }
            }
        }
    }

    /// The root of the archive file `file`, as the next folder to list; `None` when the file is
    /// no archive after all. An archive the MCP read guard keeps is never opened: opening it
    /// reads it. The error is where and why it was skipped.
    async fn enter(&mut self, file: &Dir) -> Result<Option<Dir>, (VfsPath, &'static str)> {
        let Some(limits) = self.limits else {
            return Ok(None);
        };
        let at = self.at(file, &file.path);
        if let Some(guard) = self.guard
            && guard.refuses_off_thread(&file.path).await.is_some()
        {
            self.stats.skipped += 1;
            return Err((at, "sensitive_path"));
        }
        match archive::open(&self.vfs, &self.backend, &file.path, limits).await {
            Ok(Some(scope)) => Ok(Some(Dir {
                path: PathBuf::from("/"),
                real: PathBuf::from("/"),
                depth: file.depth,
                links: 0,
                scope: Some(Arc::new(scope)),
                archive: false,
            })),
            Ok(None) => Ok(None),
            Err(error) => {
                self.stats.skipped += 1;
                Err((at, error.code()))
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

    /// `path` of the directory `dir` as a location: in its archive, or on the walk's backend.
    fn at(&self, dir: &Dir, path: &Path) -> VfsPath {
        self.located(dir.scope.as_deref(), path)
    }

    fn located(&self, scope: Option<&Scope>, path: &Path) -> VfsPath {
        VfsPath {
            backend: scope.map_or_else(|| self.backend.clone(), |scope| scope.backend.clone()),
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
        // What the ignore list hides is neither found nor entered (RC-100-92).
        let listing = self.current.as_ref()?;
        if crate::ignore_list::ignored(&self.at(&listing.dir, &listing.dir.path), &entry) {
            return None;
        }
        let listing = self.current.as_mut()?;
        let path = listing.dir.path.join(&entry.name);
        let (_, dir) = shape(&entry);
        if dir && let Some(sub) = descend(&self.query, &mut self.visited, &listing.dir, &entry) {
            listing.subdirs.push(sub);
        }
        if self.limits.is_some()
            && let Some(file) = archive_below(&self.query, &listing.dir, &entry)
        {
            listing.subdirs.push(file);
        }
        let scope = listing.dir.scope.clone();
        self.judge(path, scope, entry).await
    }

    /// The entry at `path` (in the archive `scope`, if any) reported when it matches: by name,
    /// the filters, and its content.
    async fn judge(
        &mut self,
        path: PathBuf,
        scope: Option<Arc<Scope>>,
        entry: Entry,
    ) -> Option<SearchEvent> {
        let (file, dir) = shape(&entry);
        if !self.query.names.matches(&entry.name, dir)
            || !self.query.filters.admits(&entry, self.now)
        {
            return None;
        }
        let at = self.located(scope.as_deref(), &path);
        if let Some(content) = &self.query.content {
            let link = matches!(entry.kind, EntryKind::Symlink { .. });
            if !file || (link && !self.query.follow_symlinks) {
                return None;
            }
            // A guarded file is never read, so neither its presence among the hits nor its
            // absence tells about its contents: it only counts as skipped. An archive's entry is
            // judged by its name.
            let guard = self.guard.map(|guard| {
                if scope.is_some() {
                    guard.by_name()
                } else {
                    guard
                }
            });
            if let Some(guard) = guard
                && guard.refuses_off_thread(&path).await.is_some()
            {
                self.stats.skipped += 1;
                return Some(SearchEvent::Skipped {
                    path: at,
                    code: "sensitive_path",
                });
            }
            let vfs = scope.as_ref().map_or(&self.vfs, |scope| &scope.vfs);
            let scan = scan_file(&**vfs, &path, content, &mut self.buf, &self.cancel).await;
            match scan {
                Ok(Scan::Found) if !content.not_containing => {}
                Ok(Scan::Absent) if content.not_containing => {}
                Ok(_) => return None,
                Err(error) => {
                    self.stats.skipped += 1;
                    return Some(SearchEvent::Skipped {
                        path: at,
                        code: error.code(),
                    });
                }
            }
        }
        self.stats.hits += 1;
        Some(SearchEvent::Hit(SearchHit { path: at, entry }))
    }
}

/// The archive file `entry` of `parent` to search like a folder, if it is one by its name and
/// the depth limit and the directory exclusions allow it; never one inside an archive.
fn archive_below(query: &Compiled, parent: &Dir, entry: &Entry) -> Option<Dir> {
    let depth = parent.depth + 1;
    let wanted = parent.scope.is_none()
        && matches!(entry.kind, EntryKind::File)
        && archive::candidate(&entry.name)
        && !query.max_depth.is_some_and(|max| depth > max)
        && query.names.enters(&entry.name);
    wanted.then(|| Dir {
        path: parent.path.join(&entry.name),
        real: parent.real.join(&entry.name),
        depth,
        links: parent.links,
        scope: None,
        archive: true,
    })
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
            scope: parent.scope.clone(),
            archive: false,
        }),
        // An archive shows its links and never follows them.
        EntryKind::Symlink { target, .. } if query.follow_symlinks && parent.scope.is_none() => {
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
                scope: None,
                archive: false,
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
