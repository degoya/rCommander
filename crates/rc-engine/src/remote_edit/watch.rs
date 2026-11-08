//! How a save of the local copy is seen: its size and modification time are read once a second
//! ([`POLL`]), and a new stamp counts once two reads in a row found it - a stand that holds for
//! one read only (a program still writing, an editor that saves through a temporary name) starts
//! nothing. A save is so seen at most two seconds after it, without a file watcher (RC-110-02
//! may take this place later).

use std::time::Duration;

use super::Stamp;

/// How often each copy is looked at.
pub(crate) const POLL: Duration = Duration::from_secs(1);

/// The stamp a copy was last in step with the server at, and a new one seen once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Watch {
    /// The copy as fetched, or as it was when its last save was seen.
    synced: Option<Stamp>,
    /// A stamp seen by one read, waiting for the next to confirm it.
    seen: Option<Stamp>,
}

impl Watch {
    /// A watch of a copy that is at `synced` now.
    pub(crate) fn new(synced: Option<Stamp>) -> Self {
        Self { synced, seen: None }
    }

    /// The stamp the copy is in step at.
    #[cfg(test)]
    pub(crate) fn synced(&self) -> Option<Stamp> {
        self.synced
    }

    /// One read of the copy: `now` its stamp, `None` while it cannot be read (an editor that
    /// replaces the file). Whether a save is seen with this read.
    pub(crate) fn observe(&mut self, now: Option<Stamp>) -> bool {
        let Some(now) = now else {
            self.seen = None;
            return false;
        };
        if Some(now) == self.synced {
            self.seen = None;
            return false;
        }
        if self.seen == Some(now) {
            self.synced = Some(now);
            self.seen = None;
            return true;
        }
        self.seen = Some(now);
        false
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
