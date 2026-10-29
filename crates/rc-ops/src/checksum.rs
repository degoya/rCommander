//! Checksum files, their check, split and combine as jobs (RC-020-05): `rc-checksum` does the
//! work, the job passes its pause and cancel on to the run's [`rc_checksum::Control`] and reads
//! the run's counts into its progress - the file at work with its own bytes too (RC-010-07).
//!
//! Nobody is asked: the run writes only new files, so there is no conflict, and a failure ends
//! the whole run - after `rc-checksum` removed what it had written - as the job's failure with
//! the run's error code. A line of a checksum file that does not check out is no failure but a
//! line of the report. What the run found is the snapshot's [`JobReport`], its lists capped at
//! [`MAX_REPORT_LINES`] so a finished job the queue keeps holds no unbounded list.

use std::{future::Future, sync::Arc, time::Duration};

use rc_checksum::{
    ChecksumError, CombineReport, CreateReport, SplitReport, VerifyReport, VerifyStatus,
};
use rc_vfs::{BackendId, VfsPath};
use serde::{Deserialize, Serialize};

use crate::{
    Failure, JobKind,
    runner::{Runner, Stop},
};

/// How often a running checksum job passes its flags on and publishes its progress.
const POLL: Duration = Duration::from_millis(100);

/// The most lines (files hashed, lines checked) a report keeps.
pub const MAX_REPORT_LINES: usize = 10_000;

/// What a checksum, split or combine job found and wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobReport {
    /// The checksum files written; `omitted` files hashed beyond the first
    /// [`MAX_REPORT_LINES`] are not listed (nor their checksum files).
    ChecksumsCreated {
        report: CreateReport,
        #[serde(default)]
        omitted: usize,
    },
    /// Every line's result, the summary over all of them; beyond [`MAX_REPORT_LINES`] the lines
    /// that did not check out are kept first, and `omitted` says how many are not listed.
    ChecksumsVerified {
        report: VerifyReport,
        #[serde(default)]
        omitted: usize,
    },
    Split {
        report: SplitReport,
    },
    Combined {
        report: CombineReport,
    },
}

impl Runner {
    /// Runs a `ChecksumCreate`, `ChecksumVerify`, `Split` or `Combine` job.
    pub(crate) async fn checksum(&mut self, kind: &JobKind) -> Result<(), Stop> {
        let at = match kind {
            JobKind::ChecksumCreate { dir, .. } => dir,
            JobKind::ChecksumVerify { file } => file,
            JobKind::Split { source, .. } => source,
            JobKind::Combine { first, .. } => first,
            // `Runner::run` sends no other job here.
            _ => return Ok(()),
        };
        // A job paused or cancelled before its turn came waits or stops here.
        self.checkpoint().await?;
        self.progress.current = Some(at.clone());
        let control = rc_checksum::Control::new();
        // An MCP job reads no file its read guard refuses, each judged as it is hashed.
        let control = match &self.read_guard {
            Some(guard) => control.with_guard(guard.on(&self.src_id)),
            None => control,
        };
        let (src, dst) = (Arc::clone(&self.src), Arc::clone(&self.dst));
        let result = match kind {
            JobKind::ChecksumCreate {
                dir,
                names,
                algorithm,
                layout,
            } => {
                let work =
                    rc_checksum::create(&src, &dir.path, names, *algorithm, layout, &control);
                self.drive(&at.backend, &control, work).await.map(created)
            }
            JobKind::ChecksumVerify { file } => {
                let work = rc_checksum::verify(&src, &file.path, &control);
                self.drive(&at.backend, &control, work).await.map(verified)
            }
            JobKind::Split {
                source,
                target,
                part_size,
            } => {
                let work = rc_checksum::split(
                    &src,
                    &source.path,
                    &dst,
                    &target.path,
                    *part_size,
                    &control,
                );
                let done = self.drive(&at.backend, &control, work).await;
                done.map(|report| JobReport::Split { report })
            }
            JobKind::Combine {
                first,
                target,
                name,
            } => {
                let work = rc_checksum::combine(
                    &src,
                    &first.path,
                    &dst,
                    &target.path,
                    name.as_deref(),
                    &control,
                );
                let done = self.drive(&at.backend, &control, work).await;
                done.map(|report| JobReport::Combined { report })
            }
            _ => return Ok(()),
        };
        match result {
            Ok(report) => {
                self.job.update(|snapshot| snapshot.report = Some(report));
                Ok(())
            }
            Err(ChecksumError::Cancelled) => Err(Stop::Cancelled),
            Err(error) => Err(Stop::Failed(Failure {
                path: at.clone(),
                target: None,
                code: error.code().to_owned(),
                message: error.to_string(),
            })),
        }
    }

    /// Runs `work` to its end, passing the job's pause and cancel on to `control` and its
    /// counts into the progress; the files it reads are on `backend`.
    async fn drive<T>(
        &mut self,
        backend: &BackendId,
        control: &rc_checksum::Control,
        work: impl Future<Output = Result<T, ChecksumError>>,
    ) -> Result<T, ChecksumError> {
        // Its own handle, so the futures below borrow nothing of `self`.
        let job = Arc::clone(&self.job);
        let mut work = std::pin::pin!(work);
        let mut poll = tokio::time::interval(POLL);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut cancelled = false;
        loop {
            tokio::select! {
                result = &mut work => {
                    self.count(backend, control);
                    self.tick(true);
                    return result;
                }
                () = job.control.cancelled(), if !cancelled => {
                    cancelled = true;
                    control.cancel();
                }
                _ = poll.tick() => {
                    if job.control.is_paused() {
                        control.pause();
                    } else {
                        control.resume();
                    }
                    self.count(backend, control);
                    self.tick(false);
                }
            }
        }
    }

    /// The run's counts as the job's progress; the file at work, once the run has one, on
    /// `backend`.
    fn count(&mut self, backend: &BackendId, control: &rc_checksum::Control) {
        let progress = &mut self.progress;
        progress.files_total = control.files_total();
        progress.files_done = control.files_done();
        progress.bytes_total = control.bytes_total();
        progress.bytes_done = control.bytes_done();
        if let Some(path) = control.current_file() {
            progress.current = Some(VfsPath {
                backend: backend.clone(),
                path,
            });
            progress.file_bytes_total = control.file_bytes_total();
            progress.file_bytes_done = control.file_bytes_done();
        }
    }
}

/// A create report with at most [`MAX_REPORT_LINES`] files and checksum files listed.
pub(crate) fn created(mut report: CreateReport) -> JobReport {
    let omitted = report.files.len().saturating_sub(MAX_REPORT_LINES);
    report.files.truncate(MAX_REPORT_LINES);
    report.written.truncate(MAX_REPORT_LINES);
    JobReport::ChecksumsCreated { report, omitted }
}

/// A check's report with at most [`MAX_REPORT_LINES`] lines, in their order: the lines that did
/// not check out first, then as many good ones as there is room for. The summary stays whole.
pub(crate) fn verified(mut report: VerifyReport) -> JobReport {
    let total = report.entries.len();
    if total > MAX_REPORT_LINES {
        let problems = report
            .entries
            .iter()
            .filter(|entry| entry.status != VerifyStatus::Ok)
            .count();
        let mut room_ok = MAX_REPORT_LINES.saturating_sub(problems);
        let mut room_problems = MAX_REPORT_LINES;
        report.entries.retain(|entry| {
            let room = if entry.status == VerifyStatus::Ok {
                &mut room_ok
            } else {
                &mut room_problems
            };
            let keep = *room > 0;
            *room = room.saturating_sub(1);
            keep
        });
    }
    let omitted = total - report.entries.len();
    JobReport::ChecksumsVerified { report, omitted }
}

#[cfg(test)]
#[path = "checksum_tests.rs"]
mod tests;
