//! When a watched panel is read again (RC-110-02): the pure rule, with the clock passed in.
//!
//! A change is read [`QUIET`] after the last one, so a burst is one reading. A flood that never
//! falls quiet is read at most once per gap: the gap is [`MIN_GAP`], or twice as long as the last
//! reading took when that is more, counted from the reading's end - a folder that takes 3 s to
//! list is read at most every 6 s after the last reading ended, so reading takes at most a third
//! of the time. Fixed values, no setting (owner, 2026-11-07).

use std::time::{Duration, Instant};

/// How long after the last change a panel is read.
pub(crate) const QUIET: Duration = Duration::from_millis(200);
/// The shortest pause from the end of one reading to the start of the next.
pub(crate) const MIN_GAP: Duration = Duration::from_secs(1);

/// The changes of one panel not yet read, and its last reading.
#[derive(Debug, Default, Clone)]
pub(crate) struct Coalescer {
    /// The first change since the last reading began.
    first: Option<Instant>,
    /// The latest change.
    last: Option<Instant>,
    /// A reading runs since then.
    reading: Option<Instant>,
    /// When the last reading ended, and how long it took.
    ended: Option<(Instant, Duration)>,
}

impl Coalescer {
    /// A change at `now`.
    pub(crate) fn changed(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// A change waits for a reading.
    pub(crate) fn pending(&self) -> bool {
        self.first.is_some()
    }

    /// The pause after the last reading: [`MIN_GAP`] or twice its time.
    fn gap(&self) -> Duration {
        self.ended
            .map_or(MIN_GAP, |(_, took)| MIN_GAP.max(took.saturating_mul(2)))
    }

    /// When the panel is to be read; `None` when nothing waits or a reading still runs.
    pub(crate) fn due(&self) -> Option<Instant> {
        if self.reading.is_some() {
            return None;
        }
        let (first, last) = (self.first?, self.last?);
        let gap = self.gap();
        // Quiet for a moment, or a flood waited on long enough.
        let settled = (last + QUIET).min(first + gap);
        Some(match self.ended {
            Some((end, _)) => settled.max(end + gap),
            None => settled,
        })
    }

    /// A reading begins at `now`: the changes so far are in it.
    pub(crate) fn started(&mut self, now: Instant) {
        self.first = None;
        self.last = None;
        self.reading = Some(now);
    }

    /// The reading begun last ended at `now`.
    pub(crate) fn finished(&mut self, now: Instant) {
        if let Some(start) = self.reading.take() {
            self.ended = Some((now, now.saturating_duration_since(start)));
        }
    }
}

#[cfg(test)]
#[path = "coalesce_tests.rs"]
mod tests;
