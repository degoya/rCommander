//! `files.search`, paged, and the streaming search the find-files dialog runs - which alone may
//! take its candidates from Everything (RC-100-93).

use std::{
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use rc_vfs::{BackendId, Vfs, VfsError};
use tokio::sync::mpsc;
use tracing::warn;

use super::panel::resolve_path;
use crate::recover::LockOrRecover as _;
use crate::{
    Engine, EngineError, Outcome, PanelId, read_guard,
    search::{
        Finder, SearchCancel, SearchEvent, SearchHit, SearchQuery, Walk,
        everything::{self, Everything, EverythingError},
        pages::{PageKey, Pages},
    },
};

/// Hits per page when the caller names no limit.
pub const DEFAULT_PAGE: usize = 100;
/// Hits per page at most.
pub const MAX_PAGE: usize = 1000;
/// Start folders per search at most.
const MAX_ROOTS: usize = 64;
/// Events a slow reader of a streaming search may fall behind before the walk waits.
const EVENT_BUFFER: usize = 256;

/// A walk of `files.search` kept for its next page, and that page's first hit: the one that did
/// not fit on the page before.
pub(crate) struct PagedWalk {
    walk: Walk,
    first: SearchHit,
}

/// The walks of `files.search` waiting for their next page.
pub(crate) type SearchPages = Pages<PagedWalk>;

/// A search running in the background: its events, and the switch that stops it. Dropping
/// the handle stops the search too.
#[derive(Debug)]
pub struct SearchHandle {
    events: mpsc::Receiver<SearchEvent>,
    cancel: SearchCancel,
}

impl SearchHandle {
    /// The next event; `None` once [`SearchEvent::Finished`] was delivered.
    pub async fn next(&mut self) -> Option<SearchEvent> {
        self.events.recv().await
    }

    /// Stops the search; [`SearchEvent::Finished`] with `cancelled` follows.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// A switch that stops the search from elsewhere (a dialog's Cancel button).
    #[must_use]
    pub fn canceller(&self) -> SearchCancel {
        self.cancel.clone()
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Engine {
    /// `files.search`: one page of hits. A page that ends with more hits keeps its walk for the
    /// next ([`SearchPages`]); the cursor carries the offset, a fingerprint of the search it
    /// belongs to and the kept walk's id. The next page goes on with that walk, or - when it
    /// went - runs the walk again from the start and skips the hits of the pages before.
    pub(crate) async fn search_files(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
        query: &SearchQuery,
        limit: Option<usize>,
        cursor: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let (vfs, backend, roots) = self.search_roots(panel, paths).await?;
        let fingerprint = fingerprint(&roots, query);
        let (offset, kept) =
            cursor.map_or(Ok((0, None)), |cursor| cursor_of(cursor, fingerprint))?;
        let limit = limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
        let guard = read_guard::current(&backend);
        let key = |offset| PageKey {
            fingerprint,
            guarded: guard.is_some(),
            offset,
        };
        let resumed = kept.and_then(|id| self.search_pages().take(id, key(offset), Instant::now()));
        let (mut walk, mut hits, mut seen) = match resumed {
            Some(paged) => (paged.walk, vec![paged.first], offset + 1),
            None => {
                let walk = Walk::new(
                    vfs,
                    backend,
                    roots,
                    query.compile()?,
                    SearchCancel::default(),
                )
                .guarded(guard);
                (self.with_archive_limits(walk).await, Vec::new(), 0)
            }
        };
        let mut first_of_next = None;
        while let Some(event) = walk.next().await {
            if let SearchEvent::Hit(hit) = event {
                if seen >= offset {
                    if hits.len() == limit {
                        first_of_next = Some(hit);
                        break;
                    }
                    hits.push(hit);
                }
                seen += 1;
            }
        }
        let stats = walk.stats();
        let next_cursor = first_of_next.map(|first| {
            let next = offset + limit;
            let id = self
                .search_pages()
                .keep(key(next), PagedWalk { walk, first }, Instant::now());
            format!("{next}-{fingerprint:016x}-{id:x}")
        });
        Ok(Outcome::SearchResults {
            hits,
            next_cursor,
            stats,
        })
    }

    /// `walk` opening archives as the engine opens them, with `archive.decoder_memory_mib`, when
    /// it enters any.
    async fn with_archive_limits(&self, walk: Walk) -> Walk {
        if !walk.enters_archives() {
            return walk;
        }
        self.read_archive_limits().await;
        walk.with_archive_limits(self.inner.archives.limits())
    }

    fn search_pages(&self) -> std::sync::MutexGuard<'_, SearchPages> {
        self.inner
            .search_pages
            .lock_or_recover("search.search_pages")
    }

    /// Starts a search in the background; its hits and progress arrive on the handle as they
    /// are found. `paths` and `query` mean what they mean for `files.search`.
    ///
    /// # Errors
    ///
    /// A start folder is missing or no folder, too many are given, or a pattern is invalid.
    pub async fn start_search(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
        query: &SearchQuery,
    ) -> Result<SearchHandle, EngineError> {
        self.start_search_with(panel, paths, query, None).await
    }

    /// [`Engine::start_search`], the candidates asked of `everything` when one is given and
    /// can be asked (RC-100-93): start folders on the machine's own drives, masks it takes, no
    /// archives searched. Everything that does not answer leaves the search to walk the folders
    /// as without it; a stop while it is asked ends the search.
    ///
    /// # Errors
    ///
    /// As [`Engine::start_search`].
    pub async fn start_search_with(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
        query: &SearchQuery,
        everything: Option<Arc<dyn Everything>>,
    ) -> Result<SearchHandle, EngineError> {
        let (vfs, backend, roots) = self.search_roots(panel, paths).await?;
        let cancel = SearchCancel::default();
        // Taken here: the task the walk runs in below does not inherit the dispatch's guard.
        let guard = read_guard::current(&backend);
        let asked = everything
            .filter(|_| {
                !query.archives
                    && backend == BackendId::Local
                    && roots.iter().all(|root| everything::indexed(root))
            })
            .and_then(|everything| {
                everything::expression(query, &roots).map(|search| (everything, search))
            });
        let finder = query
            .duplicates
            .map(|duplicates| Finder::new(Arc::clone(&vfs), duplicates, cancel.clone(), guard));
        let walk = Walk::new(vfs, backend, roots, query.compile()?, cancel.clone()).guarded(guard);
        let mut walk = self.with_archive_limits(walk).await;
        let (sender, events) = mpsc::channel(EVENT_BUFFER);
        let asking = cancel.clone();
        tokio::spawn(async move {
            if let Some((everything, search)) = asked {
                match everything::ask(everything, search, asking).await {
                    Ok(candidates) => walk = walk.over_candidates(candidates),
                    // The walk finishes as cancelled at its first step.
                    Err(EverythingError::Cancelled) => {}
                    Err(error) => warn!(%error, "the search walks the folders without Everything"),
                }
            }
            if let Some(finder) = finder {
                stream_duplicates(finder, walk, sender).await;
                return;
            }
            while let Some(event) = walk.next().await {
                if sender.send(event).await.is_err() {
                    break;
                }
            }
        });
        Ok(SearchHandle { events, cancel })
    }

    /// The start folders, resolved against the panel's folder (which is the one start folder
    /// when `paths` is empty) and checked to be folders - on their own backend: a drive named
    /// from a server's panel is the machine's (RC-080-42). One search walks one backend, so
    /// start folders on two are refused.
    pub(super) async fn search_roots(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
    ) -> Result<(Arc<dyn Vfs>, BackendId, Vec<PathBuf>), EngineError> {
        if paths.len() > MAX_ROOTS {
            return Err(EngineError::InvalidSearch {
                reason: format!("at most {MAX_ROOTS} start folders"),
            });
        }
        let id = self.resolve(panel);
        let current = self.state().panel(id).path.clone();
        let roots = if paths.is_empty() {
            vec![current.clone()]
        } else {
            paths
                .iter()
                .map(|path| resolve_path(&current, path))
                .collect()
        };
        let backend = roots
            .first()
            .map_or_else(|| current.backend.clone(), |root| root.backend.clone());
        if roots.iter().any(|root| root.backend != backend) {
            return Err(EngineError::InvalidSearch {
                reason: "the start folders lie on different file systems; search each on its own"
                    .to_owned(),
            });
        }
        let Some(first) = roots.first() else {
            return Err(EngineError::InvalidSearch {
                reason: "no start folder".to_owned(),
            });
        };
        let vfs = self.vfs(first)?;
        for root in &roots {
            if !vfs.stat(&root.path).await?.is_dir_like() {
                return Err(VfsError::NotADirectory {
                    path: root.path.clone(),
                }
                .into());
            }
        }
        let roots = roots.into_iter().map(|root| root.path).collect();
        Ok((vfs, backend, roots))
    }
}

/// A duplicate search streamed (RC-100-98): its progress as it goes, then its groups, then the
/// end.
async fn stream_duplicates(finder: Finder, walk: Walk, sender: mpsc::Sender<SearchEvent>) {
    let report = finder.streaming(sender.clone()).run(walk).await;
    for group in report.groups {
        if sender.send(SearchEvent::Duplicates(group)).await.is_err() {
            return;
        }
    }
    let finished = SearchEvent::Finished {
        stats: report.stats,
        cancelled: report.cancelled,
    };
    let _ = sender.send(finished).await;
}

/// The offset a cursor carries and the id of its kept walk, if it belongs to the search of
/// `fingerprint`. A cursor without an id (an older one) is good too.
pub(super) fn cursor_of(
    cursor: &str,
    fingerprint: u64,
) -> Result<(usize, Option<u64>), EngineError> {
    let mut parts = cursor.split('-');
    let offset = parts.next().and_then(|offset| offset.parse().ok());
    let print = parts
        .next()
        .and_then(|print| u64::from_str_radix(print, 16).ok());
    let kept = match parts.next() {
        Some(id) => u64::from_str_radix(id, 16).ok().map(Some),
        None => Some(None),
    };
    match (offset, print, kept, parts.next()) {
        (Some(offset), Some(print), Some(kept), None) if print == fingerprint => Ok((offset, kept)),
        _ => Err(EngineError::InvalidSearch {
            reason: "the cursor belongs to another search".to_owned(),
        }),
    }
}

/// A hash of the start folders and the query that stays the same from run to run (FNV-1a;
/// the std hasher is seeded per process).
pub(super) fn fingerprint(roots: &[PathBuf], query: &SearchQuery) -> u64 {
    let mut hasher = Fnv(0xcbf2_9ce4_8422_2325);
    roots.hash(&mut hasher);
    query.hash(&mut hasher);
    hasher.finish()
}

struct Fnv(u64);

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
