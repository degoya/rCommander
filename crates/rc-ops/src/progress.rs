//! How far a job has got: counts, speed and the time left, and the throttle that keeps the
//! events to about ten a second.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

/// A job's progress. Totals come from counting before the work starts; skipped and failed
/// files count as done, so a finished job stands at its totals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub files_total: u64,
    pub files_done: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    /// The file at work.
    pub current: Option<VfsPath>,
    /// The bytes of the file at work done so far - from the offset when it is continued - and
    /// its size as counted; both start again with each file (RC-010-07).
    #[serde(default)]
    pub file_bytes_done: u64,
    #[serde(default)]
    pub file_bytes_total: u64,
    /// Over the last few seconds; 0 before the first measurement.
    pub bytes_per_second: u64,
    /// The time left at that speed; `None` while the speed is unknown.
    pub eta_seconds: Option<u64>,
}

/// How far back the speed looks.
const WINDOW: Duration = Duration::from_secs(5);

/// The speed over a sliding window of samples, each `N` counts at one moment: a job's bytes
/// here, a whole operation's bytes and files in the app's file operations.
#[derive(Debug)]
pub struct Meter<const N: usize = 1> {
    samples: VecDeque<(Instant, [u64; N])>,
}

impl<const N: usize> Default for Meter<N> {
    fn default() -> Self {
        Self {
            samples: VecDeque::new(),
        }
    }
}

impl<const N: usize> Meter<N> {
    /// Records the counts `done` at `now`; samples older than the window drop out, the last
    /// two always stay.
    pub fn record(&mut self, now: Instant, done: [u64; N]) {
        self.samples.push_back((now, done));
        while self.samples.len() > 2
            && self
                .samples
                .front()
                .is_some_and(|(at, _)| now.saturating_duration_since(*at) > WINDOW)
        {
            self.samples.pop_front();
        }
    }

    /// Each count per second over the window; `None` before two samples a moment apart.
    #[must_use]
    pub fn rates(&self) -> Option<[f64; N]> {
        let ((first_at, first), (last_at, last)) = (self.samples.front()?, self.samples.back()?);
        let seconds = last_at.saturating_duration_since(*first_at).as_secs_f64();
        if seconds < 0.05 {
            return None;
        }
        Some(std::array::from_fn(|at| {
            last[at].saturating_sub(first[at]) as f64 / seconds
        }))
    }

    /// Forgets the samples: a pause must not lower the speed after it.
    pub fn reset(&mut self) {
        self.samples.clear();
    }
}

impl Meter {
    /// Records `bytes_done` at `now`; the speed in bytes per second.
    pub(crate) fn sample(&mut self, now: Instant, bytes_done: u64) -> u64 {
        self.record(now, [bytes_done]);
        // Whole bytes per second; precision below one byte does not matter.
        self.rates().map_or(0, |[speed]| speed as u64)
    }
}

impl Progress {
    /// The file at `path` of `size` bytes is at work: its own bytes start at 0.
    pub(crate) fn begin_file(&mut self, path: VfsPath, size: u64) {
        self.current = Some(path);
        self.file_bytes_total = size;
        self.file_bytes_done = 0;
    }

    /// `bytes` more of the file at work, and so of the job.
    pub(crate) fn count(&mut self, bytes: u64) {
        self.bytes_done += bytes;
        self.file_bytes_done += bytes;
    }

    /// The file at work starts over: the job's bytes back to `start`, where they stood when it
    /// began, the file's back to 0.
    pub(crate) fn restart_file(&mut self, start: u64) {
        self.bytes_done = start;
        self.file_bytes_done = 0;
    }

    /// Updates speed and time left from `meter` at `now`.
    pub(crate) fn measure(&mut self, meter: &mut Meter, now: Instant) {
        self.bytes_per_second = meter.sample(now, self.bytes_done);
        self.eta_seconds = (self.bytes_per_second > 0)
            .then(|| self.bytes_total.saturating_sub(self.bytes_done) / self.bytes_per_second);
    }
}

/// Lets an event through at most once per interval.
#[derive(Debug)]
pub(crate) struct Throttle {
    interval: Duration,
    last: Option<Instant>,
}

impl Throttle {
    pub(crate) fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// Whether an event may go out at `now`; if so, the interval starts again.
    pub(crate) fn ready(&mut self, now: Instant) -> bool {
        let due = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= self.interval);
        if due {
            self.last = Some(now);
        }
        due
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
