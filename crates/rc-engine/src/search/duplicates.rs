//! Duplicate files, as Total Commander's "Find duplicate files" finds them (RC-100-98): the files
//! the search finds, grouped by the same size - with the same name, the same contents, or both.
//!
//! The walk of the search collects the matching files first (at most [`MAX_FLAT_ENTRIES`], what
//! a panel can be fed; past that the result says it was cut). They are grouped by size and,
//! when asked, by name - without case where the file system ignores it. Only groups of two or
//! more go on: their files are asked for their identity, so two names of one file (a hard link)
//! are never taken for a copy, and - for the same contents - read: first the first 64 KiB of each
//! ([`hash`]), then, among those still alike, the whole file; BLAKE3 both times. A file the MCP
//! read guard keeps is never read: it is left out and counted as skipped. The groups come only
//! after the walk, sorted by the first criterion asked for. A stop ends the run at the next file
//! or chunk, with the groups finished before it.

mod hash;

use std::{
    collections::{HashMap, HashSet},
    ops::Not,
    sync::Arc,
    time::{Duration, Instant},
};

use rc_vfs::{Entry, EntryKind, FileId, Vfs, VfsPath};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use self::hash::{HEAD, Stop, digest};
use super::{SearchCancel, SearchEvent, SearchHit, SearchStats, Walk};
use crate::{EngineError, MAX_FLAT_ENTRIES, read_guard::ReadGuard};

/// How often a [`SearchEvent::Progress`] is reported at most while files are read.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// What makes two files duplicates: always the same size (the contents include it), and
/// optionally the same name. A query without size and contents finds nothing and is refused.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct DuplicateQuery {
    /// The same name, without case where the file system ignores it.
    pub name: bool,
    /// The same size.
    pub size: bool,
    /// The same contents (and so the same size).
    pub content: bool,
}

impl DuplicateQuery {
    /// Whether the query can find anything: by size or by contents, with or without the name.
    ///
    /// # Errors
    ///
    /// [`EngineError::InvalidSearch`] for a query without size and contents.
    pub(crate) fn check(self) -> Result<(), EngineError> {
        if self.size || self.content {
            Ok(())
        } else {
            Err(EngineError::InvalidSearch {
                reason: "duplicates are found by size or by contents, with or without the name"
                    .to_owned(),
            })
        }
    }
}

/// One file of a [`DuplicateGroup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateFile {
    pub path: VfsPath,
    pub entry: Entry,
    /// Another name of a file listed before it in the group (a hard link): no copy.
    #[serde(default, skip_serializing_if = "Not::not")]
    pub hard_link: bool,
}

/// Files alike by the criteria asked, at least two of them distinct files, in walk order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// The bytes of each file.
    pub size: u64,
    pub files: Vec<DuplicateFile>,
}

/// What a run found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DuplicateReport {
    pub(crate) groups: Vec<DuplicateGroup>,
    /// The walk's counts; `hits` are the files in the groups.
    pub(crate) stats: SearchStats,
    /// More files matched than are compared ([`MAX_FLAT_ENTRIES`]).
    pub(crate) truncated: bool,
    pub(crate) cancelled: bool,
}

/// A file of a group being formed: where it is, what it is, and its identity when the backend
/// tells one (only a file with several names has one).
struct Member {
    hit: SearchHit,
    id: Option<FileId>,
}

/// One run over the files a walk finds.
pub(crate) struct Finder {
    vfs: Arc<dyn Vfs>,
    query: DuplicateQuery,
    cancel: SearchCancel,
    guard: Option<ReadGuard>,
    /// Where progress and skipped files go while the run streams; `None` for a page.
    events: Option<mpsc::Sender<SearchEvent>>,
    /// Files compared at most.
    limit: usize,
    stats: SearchStats,
    buf: Vec<u8>,
    last_progress: Instant,
}

impl Finder {
    /// A run reading through `vfs` (the walk's own backend), stopped by `cancel` (the walk's
    /// too), never reading what `guard` keeps.
    pub(crate) fn new(
        vfs: Arc<dyn Vfs>,
        query: DuplicateQuery,
        cancel: SearchCancel,
        guard: Option<ReadGuard>,
    ) -> Self {
        Self {
            vfs,
            query,
            cancel,
            guard,
            events: None,
            limit: MAX_FLAT_ENTRIES,
            stats: SearchStats::default(),
            buf: Vec::new(),
            last_progress: Instant::now(),
        }
    }

    /// The run reporting its progress and skipped files on `events`.
    pub(crate) fn streaming(mut self, events: mpsc::Sender<SearchEvent>) -> Self {
        self.events = Some(events);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Collects the files `walk` finds and groups them.
    pub(crate) async fn run(mut self, mut walk: Walk) -> DuplicateReport {
        let mut found = Vec::new();
        let mut truncated = false;
        let mut cancelled = false;
        while let Some(event) = walk.next().await {
            match event {
                SearchEvent::Hit(hit) => {
                    if hit.entry.kind != EntryKind::File {
                        continue;
                    }
                    if found.len() >= self.limit {
                        truncated = true;
                        break;
                    }
                    found.push(hit);
                }
                SearchEvent::Finished {
                    cancelled: stopped, ..
                } => cancelled = stopped,
                SearchEvent::Duplicates(_) => {}
                event @ (SearchEvent::Progress { .. } | SearchEvent::Skipped { .. }) => {
                    self.send(event).await;
                }
            }
        }
        self.stats = walk.stats();
        self.stats.hits = 0;
        let mut groups = Vec::new();
        if !cancelled {
            let fold = !self.vfs.capabilities().case_sensitive;
            for alike in self.alike(found, fold) {
                if self.cancel.is_cancelled() {
                    cancelled = true;
                    break;
                }
                let members = self.identified(alike).await;
                let sets = if self.query.content {
                    match self.by_content(members).await {
                        Some(sets) => sets,
                        None => {
                            cancelled = true;
                            break;
                        }
                    }
                } else {
                    vec![members]
                };
                groups.extend(sets.into_iter().filter_map(group));
            }
        }
        self.sort(&mut groups, !self.vfs.capabilities().case_sensitive);
        self.stats.hits = groups.iter().map(|group| group.files.len() as u64).sum();
        self.stats.truncated = truncated;
        DuplicateReport {
            groups,
            stats: self.stats,
            truncated,
            cancelled,
        }
    }

    /// The files grouped by size, and by name when asked, in walk order; only groups of two or
    /// more.
    fn alike(&self, found: Vec<SearchHit>, fold: bool) -> Vec<Vec<SearchHit>> {
        let mut at: HashMap<(Option<String>, u64), usize> = HashMap::new();
        let mut alike: Vec<Vec<SearchHit>> = Vec::new();
        for hit in found {
            let name = self.query.name.then(|| name_key(&hit.entry.name, fold));
            let key = (name, hit.entry.size);
            match at.get(&key) {
                Some(&index) => alike[index].push(hit),
                None => {
                    at.insert(key, alike.len());
                    alike.push(vec![hit]);
                }
            }
        }
        alike.retain(|group| group.len() >= 2);
        alike
    }

    /// Each file with its identity, as far as the backend tells it.
    async fn identified(&self, hits: Vec<SearchHit>) -> Vec<Member> {
        let mut members = Vec::with_capacity(hits.len());
        for hit in hits {
            let id = match self.vfs.details(&hit.path.path).await {
                Ok(details) => details.id,
                Err(_) => None,
            };
            members.push(Member { hit, id });
        }
        members
    }

    /// `members` split by contents: by the digest of their head, then of the whole file where
    /// it is longer. `None` once the run was stopped.
    async fn by_content(&mut self, members: Vec<Member>) -> Option<Vec<Vec<Member>>> {
        let size = members.first().map_or(0, |member| member.hit.entry.size);
        let heads = self.split(members, Some(HEAD)).await?;
        if size <= HEAD {
            return Some(heads);
        }
        let mut sets = Vec::new();
        for head in heads {
            sets.extend(self.split(head, None).await?);
        }
        Some(sets)
    }

    /// `members` with at least two distinct files split by the digest of their first `limit`
    /// bytes (all of them for `None`); each file is read once, its other names follow it. A
    /// file that cannot be read, or that the read guard keeps, is left out. `None` once stopped.
    async fn split(
        &mut self,
        members: Vec<Member>,
        limit: Option<u64>,
    ) -> Option<Vec<Vec<Member>>> {
        if distinct(&members) < 2 {
            return Some(Vec::new());
        }
        let mut of_id: HashMap<FileId, String> = HashMap::new();
        let mut at: HashMap<String, usize> = HashMap::new();
        let mut sets: Vec<Vec<Member>> = Vec::new();
        for member in members {
            let known = member.id.and_then(|id| of_id.get(&id).cloned());
            let key = match known {
                Some(key) => key,
                None => match self.read(&member, limit).await {
                    Ok(Some(key)) => {
                        if let Some(id) = member.id {
                            of_id.insert(id, key.clone());
                        }
                        key
                    }
                    Ok(None) => continue,
                    Err(_) => return None,
                },
            };
            match at.get(&key) {
                Some(&index) => sets[index].push(member),
                None => {
                    at.insert(key, sets.len());
                    sets.push(vec![member]);
                }
            }
        }
        sets.retain(|set| distinct(set) >= 2);
        Some(sets)
    }

    /// The digest of one file's first `limit` bytes; `None` when it is left out (guarded,
    /// unreadable), which is reported as skipped.
    async fn read(&mut self, member: &Member, limit: Option<u64>) -> Result<Option<String>, Stop> {
        let path = &member.hit.path;
        self.progress(path).await;
        if let Some(guard) = self.guard
            && guard.refuses_off_thread(&path.path).await.is_some()
        {
            self.skip(path, "sensitive_path").await;
            return Ok(None);
        }
        match digest(&*self.vfs, &path.path, limit, &mut self.buf, &self.cancel).await {
            Ok(key) => Ok(Some(key)),
            Err(Stop::Failed(code)) => {
                self.skip(path, code).await;
                Ok(None)
            }
            Err(Stop::Cancelled) => Err(Stop::Cancelled),
        }
    }

    /// The groups in the order of the first criterion asked: by name, else by size; ties by
    /// the path of their first file.
    fn sort(&self, groups: &mut [DuplicateGroup], fold: bool) {
        let first = |group: &DuplicateGroup| {
            group
                .files
                .first()
                .map(|file| (name_key(&file.entry.name, fold), file.path.path.clone()))
                .unwrap_or_default()
        };
        if self.query.name {
            groups.sort_by_cached_key(|group| (first(group).0, group.size, first(group).1));
        } else {
            groups.sort_by_cached_key(|group| (group.size, first(group).1));
        }
    }

    async fn skip(&mut self, path: &VfsPath, code: &'static str) {
        self.stats.skipped += 1;
        self.send(SearchEvent::Skipped {
            path: path.clone(),
            code,
        })
        .await;
    }

    /// The file being read, when it is time to say so.
    async fn progress(&mut self, path: &VfsPath) {
        if self.last_progress.elapsed() >= PROGRESS_EVERY {
            self.last_progress = Instant::now();
            let event = SearchEvent::Progress {
                dir: path.parent(),
                stats: self.stats,
            };
            self.send(event).await;
        }
    }

    async fn send(&self, event: SearchEvent) {
        if let Some(events) = &self.events {
            // A dialog that went away cancels the run through the handle; nothing to do here.
            let _ = events.send(event).await;
        }
    }
}

/// The group of `members`, the other names of a file marked as hard links; `None` unless at
/// least two distinct files are left.
fn group(members: Vec<Member>) -> Option<DuplicateGroup> {
    if distinct(&members) < 2 {
        return None;
    }
    let size = members.first().map_or(0, |member| member.hit.entry.size);
    let mut seen = HashSet::new();
    let files = members
        .into_iter()
        .map(|member| DuplicateFile {
            hard_link: member.id.is_some_and(|id| !seen.insert(id)),
            path: member.hit.path,
            entry: member.hit.entry,
        })
        .collect();
    Some(DuplicateGroup { size, files })
}

/// How many distinct files `members` are: names without an identity count each.
fn distinct(members: &[Member]) -> usize {
    let mut ids = HashSet::new();
    members
        .iter()
        .filter(|member| member.id.is_none_or(|id| ids.insert(id)))
        .count()
}

/// A name as the grouping compares it: lower case where the file system ignores case.
fn name_key(name: &str, fold: bool) -> String {
    if fold {
        name.to_lowercase()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
#[path = "duplicates_tests.rs"]
mod tests;
