//! The figures of the queue window (RC-010-14, RC-010-26): how far a job is, the file a running
//! job is on with its bytes, speed and time left, the cursor job's counts, the running jobs'
//! speed together, and whether every job shows the same lane.

use rc_engine::{JobSnapshot, JobState, QueueEntry};

use crate::{app::file_ops::duration, format, i18n::t};

/// The share done, from 0 to 1: by bytes, or by files where there are no bytes.
pub(super) fn share(job: &JobSnapshot) -> f32 {
    let progress = &job.progress;
    let (done, total) = if progress.bytes_total > 0 {
        (progress.bytes_done, progress.bytes_total)
    } else {
        (progress.files_done, progress.files_total)
    };
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

/// The file a running job is on and how far it is: bytes done of all, and the speed with the time
/// left once it is measured; `None` for a job that is not on a file now.
pub(super) fn live(job: &JobSnapshot) -> Option<(String, String)> {
    let progress = &job.progress;
    let file = progress
        .current
        .as_ref()
        .filter(|_| job.state == JobState::Running)?;
    let bytes = t!(
        "queue.live",
        done = format::size(progress.bytes_done),
        total = format::size(progress.bytes_total),
    );
    let shown = match progress.eta_seconds {
        Some(eta) if progress.bytes_per_second > 0 => t!(
            "queue.detail",
            counts = bytes,
            speed = t!(
                "ops.speed",
                speed = format::size(progress.bytes_per_second),
                eta = duration(eta),
            ),
        ),
        _ => bytes,
    };
    Some((rc_engine::shown_path(file).display().to_string(), shown))
}

/// Whether every job shows the same lane (and there is at least one).
pub(super) fn single(lanes: &[String]) -> bool {
    lanes
        .first()
        .is_some_and(|first| lanes.iter().all(|lane| lane == first))
}

/// How many jobs run and their speed together, once more than one runs.
pub(super) fn running_together(entries: &[QueueEntry]) -> Option<(usize, u64)> {
    let running: Vec<&JobSnapshot> = entries
        .iter()
        .map(|entry| &entry.job)
        .filter(|job| job.state == JobState::Running)
        .collect();
    (running.len() > 1).then(|| {
        let speed = running
            .iter()
            .map(|job| job.progress.bytes_per_second)
            .sum();
        (running.len(), speed)
    })
}

/// Files and bytes, rate and time left of a job that has counted its work.
pub(super) fn detail(job: &JobSnapshot) -> Option<String> {
    let progress = &job.progress;
    if progress.files_total == 0 {
        return None;
    }
    let counts = t!(
        "ops.progress_counts",
        files_done = format::count(progress.files_done),
        files = format::count(progress.files_total),
        bytes_done = format::size(progress.bytes_done),
        bytes = format::size(progress.bytes_total),
    );
    Some(match progress.eta_seconds {
        Some(eta) if job.state == JobState::Running => t!(
            "queue.detail",
            counts = counts,
            speed = t!(
                "ops.speed",
                speed = format::size(progress.bytes_per_second),
                eta = duration(eta),
            ),
        ),
        _ => counts,
    })
}
