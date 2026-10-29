//! What the progress window shows for a whole operation (RC-010-07): the files and bytes over
//! every job of its batch, and its speed, time left and time since the start.

use rc_engine::{JobSnapshot, JobState, Progress};

use super::view::duration;
use crate::{format, i18n::t};

/// The line under the total: speed and time left of the whole operation, and the time since it
/// started.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Timing {
    pub bytes_per_second: u64,
    pub eta_seconds: Option<u64>,
    /// The time left counts jobs not counted yet by the counted ones.
    pub estimate: bool,
    pub elapsed_seconds: Option<u64>,
}

impl Timing {
    /// Speed and time left, "about" while the time left is a guess; empty before a speed.
    pub fn speed_line(&self) -> String {
        let Some(eta) = self.eta_seconds else {
            return String::new();
        };
        let speed = format::size(self.bytes_per_second);
        if self.estimate {
            t!("ops.speed_about", speed = speed, eta = duration(eta))
        } else {
            t!("ops.speed", speed = speed, eta = duration(eta))
        }
    }

    /// The figures beside the total bar: its share, and the time since the start.
    pub fn total_figures(&self, share: f32) -> String {
        let percent = format::percent(share);
        match self.elapsed_seconds {
            Some(elapsed) => t!(
                "ops.progress_total_elapsed",
                percent = percent,
                elapsed = duration(elapsed),
            ),
            None => percent,
        }
    }
}

/// `done` of `total` from 0 to 1; 0 where there is no total.
pub(super) fn ratio(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

/// Files and bytes over the jobs of one operation: the progress window's total.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Jobs whose turn has not come or that are still counting.
    uncounted: u64,
    /// The other jobs, with their totals.
    counted: u64,
    counted_files: u64,
    counted_bytes: u64,
}

impl Totals {
    pub(super) fn add(&mut self, snapshot: &JobSnapshot) {
        self.add_progress(&snapshot.progress, snapshot.state);
    }

    /// A job that is over, a completed one at its totals.
    pub(super) fn add_finished(&mut self, snapshot: &JobSnapshot) {
        let mut progress = snapshot.progress.clone();
        if snapshot.state == JobState::Completed {
            progress.files_done = progress.files_total;
            progress.bytes_done = progress.bytes_total;
        }
        self.add_progress(&progress, snapshot.state);
    }

    fn add_progress(&mut self, progress: &Progress, state: JobState) {
        self.files_done += progress.files_done;
        self.files_total += progress.files_total;
        self.bytes_done += progress.bytes_done;
        self.bytes_total += progress.bytes_total;
        if !state.is_finished()
            && (matches!(state, JobState::Queued | JobState::Counting) || progress.files_total == 0)
        {
            self.uncounted += 1;
        } else {
            self.counted += 1;
            self.counted_files += progress.files_total;
            self.counted_bytes += progress.bytes_total;
        }
    }

    /// What the jobs not counted yet are likely to add to `sum` of the counted ones: as much
    /// as a counted job on average.
    fn guess(&self, sum: u64) -> u64 {
        match self.counted {
            0 => 0,
            counted => sum.saturating_mul(self.uncounted) / counted,
        }
    }

    /// The bytes and files still to do, the jobs not counted yet guessed.
    pub fn left(&self) -> (u64, u64) {
        let bytes = self.bytes_total + self.guess(self.counted_bytes);
        let files = self.files_total + self.guess(self.counted_files);
        (
            bytes.saturating_sub(self.bytes_done),
            files.saturating_sub(self.files_done),
        )
    }

    /// Whether a job is not counted yet, so what is left is a guess.
    pub fn is_estimate(&self) -> bool {
        self.uncounted > 0
    }

    /// The share done, from 0 to 1: by bytes, or by files where there are no bytes. A job not
    /// counted yet weighs as much as a counted one on average, so the bar does not reach its
    /// end between two jobs of a batch and fall back when the next one has counted.
    pub fn share(&self) -> f32 {
        let (bytes, files) = self.left();
        if self.bytes_total > 0 {
            ratio(self.bytes_done, self.bytes_done + bytes)
        } else {
            ratio(self.files_done, self.files_done + files)
        }
    }
}
