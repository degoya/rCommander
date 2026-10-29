//! The searches of `files.search` kept between their pages (RC-080-50): a page that ends with
//! more hits leaves its walk here under the id its cursor names, and the next page goes on from
//! there instead of walking again from the start. A walk is kept for [`PAGE_TTL`] after its last
//! page and at most [`MAX_PAGED`] at once, the one used longest ago going first - limits for
//! safety, no settings. A cursor whose walk went is still good: that page walks from the start
//! and skips the pages before, as every page did before.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// How long a search waits for its next page.
pub(crate) const PAGE_TTL: Duration = Duration::from_secs(5 * 60);

/// Searches kept at once.
pub(crate) const MAX_PAGED: usize = 8;

/// What a kept search is for: the search, whether it runs under the read guard of an MCP call,
/// and the hits handed out before its next page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageKey {
    pub(crate) fingerprint: u64,
    pub(crate) guarded: bool,
    pub(crate) offset: usize,
}

struct Kept<T> {
    id: u64,
    key: PageKey,
    used: Instant,
    walk: T,
}

/// The kept searches, the one used longest ago first. Time is a parameter so the edge can be
/// tested.
pub(crate) struct Pages<T> {
    next_id: u64,
    kept: VecDeque<Kept<T>>,
}

impl<T> Default for Pages<T> {
    fn default() -> Self {
        Self {
            next_id: 1,
            kept: VecDeque::new(),
        }
    }
}

impl<T> Pages<T> {
    /// The search `id` for the page `key` asks for, taken out: no other page goes on with it
    /// until it is kept again. `None` when it went, or belongs to another search, another
    /// guard or another page.
    pub(crate) fn take(&mut self, id: u64, key: PageKey, now: Instant) -> Option<T> {
        self.prune(now);
        let at = self
            .kept
            .iter()
            .position(|kept| kept.id == id && kept.key == key)?;
        self.kept.remove(at).map(|kept| kept.walk)
    }

    /// Keeps `walk` for the page `key` names; answers the id its cursor carries.
    pub(crate) fn keep(&mut self, key: PageKey, walk: T, now: Instant) -> u64 {
        self.prune(now);
        while self.kept.len() >= MAX_PAGED {
            self.kept.pop_front();
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.kept.push_back(Kept {
            id,
            key,
            used: now,
            walk,
        });
        id
    }

    fn prune(&mut self, now: Instant) {
        self.kept
            .retain(|kept| now.saturating_duration_since(kept.used) < PAGE_TTL);
    }
}

#[cfg(test)]
#[path = "pages_tests.rs"]
mod tests;
