//! The watcher's fallback (RC-110-02): where the system cannot watch a folder - a WSL
//! distribution's share, whose changes made inside Linux Windows may never report, or a folder
//! `notify` refuses - its modification time is read once per [`EVERY`]: one `stat`, no listing.
//! Creating, deleting and renaming an entry change it; the panel is read only when it changed.
//! The clock is passed in.

use std::time::{Duration, Instant, SystemTime};

/// How often a polled folder's time is read.
pub(crate) const EVERY: Duration = Duration::from_secs(1);

/// One polled folder: its last known modification time and when to read it next.
#[derive(Debug, Clone)]
pub(crate) struct Poller {
    /// The time read last; `None` when it could not be read.
    modified: Option<SystemTime>,
    next: Instant,
}

impl Poller {
    /// A folder whose time is `modified` at `now`, read again after [`EVERY`].
    pub(crate) fn new(modified: Option<SystemTime>, now: Instant) -> Self {
        Self {
            modified,
            next: now + EVERY,
        }
    }

    /// When its time is to be read next.
    pub(crate) fn next(&self) -> Instant {
        self.next
    }

    /// Whether its time is to be read at `now`; at most once per [`EVERY`].
    pub(crate) fn wants_stat(&self, now: Instant) -> bool {
        now >= self.next
    }

    /// The time read at `now` was `modified`: whether the folder changed since the last read.
    /// A folder that stops answering changed too: its panel is read, and finds it gone.
    pub(crate) fn seen(&mut self, modified: Option<SystemTime>, now: Instant) -> bool {
        self.next = now + EVERY;
        let changed = modified != self.modified;
        self.modified = modified;
        changed
    }
}

#[cfg(test)]
#[path = "poll_tests.rs"]
mod tests;
