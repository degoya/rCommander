//! `files.find_duplicates` (RC-100-98): the duplicate search of the find-files dialog, its groups
//! page by page. The groups exist only once every file was compared, so the first page runs the
//! whole search; the rest of its groups wait for the next page as a walk of `files.search` does
//! ([`DuplicatePages`]). A cursor whose result went runs the search again and skips the groups of
//! the pages before. The search runs inside the call: cancelling the call stops it.

use std::{sync::Arc, time::Instant};

use super::search::{DEFAULT_PAGE, MAX_PAGE, cursor_of, fingerprint};
use crate::recover::LockOrRecover as _;
use crate::{
    Engine, EngineError, Outcome, PanelId, read_guard,
    search::{
        DuplicateQuery, DuplicateReport, Finder, SearchCancel, SearchQuery, Walk,
        pages::{PageKey, Pages},
    },
};

/// The results of `files.find_duplicates` waiting for their next page.
pub(crate) type DuplicatePages = Pages<Arc<DuplicateReport>>;

impl Engine {
    /// `files.find_duplicates`: one page of groups of the search `query` finds with
    /// `duplicates`.
    pub(crate) async fn find_duplicates(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
        mut query: SearchQuery,
        duplicates: DuplicateQuery,
        limit: Option<usize>,
        cursor: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        query.duplicates = Some(duplicates);
        let compiled = query.compile()?;
        let (vfs, backend, roots) = self.search_roots(panel, paths).await?;
        let fingerprint = fingerprint(&roots, &query);
        let (offset, kept) =
            cursor.map_or(Ok((0, None)), |cursor| cursor_of(cursor, fingerprint))?;
        let limit = limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
        let guard = read_guard::current(&backend);
        let key = |offset| PageKey {
            fingerprint,
            guarded: guard.is_some(),
            offset,
        };
        let kept = kept.and_then(|id| self.duplicate_pages().take(id, key(offset), Instant::now()));
        let report = match kept {
            Some(report) => report,
            None => {
                let cancel = SearchCancel::default();
                let walk = Walk::new(Arc::clone(&vfs), backend, roots, compiled, cancel.clone())
                    .guarded(guard);
                Arc::new(Finder::new(vfs, duplicates, cancel, guard).run(walk).await)
            }
        };
        let groups: Vec<_> = report
            .groups
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        let next = offset + groups.len();
        let next_cursor = (next < report.groups.len()).then(|| {
            let id = self
                .duplicate_pages()
                .keep(key(next), Arc::clone(&report), Instant::now());
            format!("{next}-{fingerprint:016x}-{id:x}")
        });
        Ok(Outcome::DuplicateGroups {
            groups,
            next_cursor,
            stats: report.stats,
        })
    }

    fn duplicate_pages(&self) -> std::sync::MutexGuard<'_, DuplicatePages> {
        self.inner
            .duplicate_pages
            .lock_or_recover("search.duplicate_pages")
    }
}

#[cfg(test)]
#[path = "duplicates_tests.rs"]
mod tests;
