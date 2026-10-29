//! How fast one operation goes as a whole (RC-010-07): the speed over the summed bytes and files
//! of every job of its batch, measured in `rc_ops`' sliding window ([`Meter`]), so it does not
//! start again with each job; the gap between two jobs counts as time. From it the time left
//! for the whole operation, and the time since it started.

use std::time::Instant;

use rc_engine::Meter;

/// The samples of one operation since it started.
#[derive(Debug)]
pub struct Pace {
    started: Instant,
    /// Bytes and files done, over the batch.
    meter: Meter<2>,
}

/// Bytes and files per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub bytes: f64,
    pub files: f64,
}

impl Pace {
    pub fn new(now: Instant) -> Self {
        Self {
            started: now,
            meter: Meter::default(),
        }
    }

    /// Records the batch at `bytes` and `files` done at `now`.
    pub fn sample(&mut self, now: Instant, bytes: u64, files: u64) {
        self.meter.record(now, [bytes, files]);
    }

    /// Forgets the samples: a pause or a question must not lower the speed after it.
    pub fn reset(&mut self) {
        self.meter.reset();
    }

    /// Whole seconds since the operation started.
    pub fn elapsed(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.started).as_secs()
    }

    /// The speed over the window; `None` before two samples a moment apart.
    pub fn rates(&self) -> Option<Rates> {
        let [bytes, files] = self.meter.rates()?;
        Some(Rates { bytes, files })
    }
}

/// The time left for `bytes` and `files` still to do at `rates`: the longer of the two, since
/// with many small files the work per file outweighs the bytes; `None` while nothing moves.
pub fn time_left(bytes: u64, files: u64, rates: Rates) -> Option<u64> {
    let by = |left: u64, rate: f64| (rate > 0.0).then(|| (left as f64 / rate) as u64);
    match (by(bytes, rates.bytes), by(files, rates.files)) {
        (Some(bytes), Some(files)) => Some(bytes.max(files)),
        (one, other) => one.or(other),
    }
}

#[cfg(test)]
#[path = "pace_tests.rs"]
mod tests;
