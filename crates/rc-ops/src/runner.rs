//! One running job: its backends, its progress, and how it asks its caller.
//!
//! The work itself is in `walk.rs` (counting), `copy.rs` (copy and move) with `stream_pair.rs`
//! (a file's two streams at once), `delete.rs` (delete and mkdir), `rename.rs` (the steps of a
//! multi-rename), `archive_test.rs` (the test of an archive) and `checksum.rs` (checksum files,
//! split, combine), each an `impl Runner` block. Every step that can fail goes through
//! [`attempt!`]: on failure the job asks (or follows `on_error`) and retries, skips or stops.

use std::{path::Path, sync::Arc, time::Instant};

use rc_vfs::{ArchivePassword, ArchiveSettings, BackendId, Vfs, VfsError, VfsPath};

use crate::{
    Answer, ConflictChoice, ErrorChoice, ErrorPolicy, Failure, JobChange, JobKind, JobOptions,
    OpsError, Progress, Question,
    conflict::{Decision, EntryInfo, decide},
    control::Cancelled,
    journal::StepJournal,
    persist::{FileMark, TransferStore},
    progress::{Meter, Throttle},
    queue::JobShared,
    resume::{Reconnect, RetryPolicy},
    stream_pair::Buffers,
    trash::Trash,
};

/// Why a job stops before its end.
#[derive(Debug)]
pub(crate) enum Stop {
    /// The caller cancelled, or answered a conflict with `Cancel`.
    Cancelled,
    /// A file failed and the answer (or `on_error`) was to stop.
    Failed(Failure),
}

impl From<Cancelled> for Stop {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}

/// How one step ends when it does not succeed: the job stops, or this one file failed.
#[derive(Debug)]
pub(crate) enum Step {
    Stop(Stop),
    Fail(OpsError),
}

impl From<Stop> for Step {
    fn from(stop: Stop) -> Self {
        Self::Stop(stop)
    }
}

impl From<Cancelled> for Step {
    fn from(_: Cancelled) -> Self {
        Self::Stop(Stop::Cancelled)
    }
}

impl From<OpsError> for Step {
    fn from(error: OpsError) -> Self {
        Self::Fail(error)
    }
}

impl From<VfsError> for Step {
    fn from(error: VfsError) -> Self {
        Self::Fail(error.into())
    }
}

/// Runs `$op` (a `Result<T, Step>` expression, evaluated again on each retry) until it
/// succeeds - `Some(value)` - or the failure is skipped - `None`; a stop returns from the
/// enclosing function, which returns `Result<_, Stop>`.
macro_rules! attempt {
    ($runner:expr, $path:expr, $op:expr) => {
        loop {
            match $op {
                Ok(value) => break Some(value),
                Err($crate::runner::Step::Stop(stop)) => return Err(stop),
                Err($crate::runner::Step::Fail(error)) => {
                    match $runner.failed($path, error).await? {
                        $crate::ErrorChoice::Skip => break None,
                        _ => continue,
                    }
                }
            }
        }
    };
}
pub(crate) use attempt;

/// Fixed for the life of the job.
pub(crate) struct Setup {
    pub(crate) job: Arc<JobShared>,
    pub(crate) options: JobOptions,
    pub(crate) buffer_size: usize,
    pub(crate) progress_interval: std::time::Duration,
    pub(crate) src_id: BackendId,
    pub(crate) src: Arc<dyn Vfs>,
    pub(crate) trash: Option<Arc<dyn Trash>>,
    pub(crate) dst: Arc<dyn Vfs>,
    pub(crate) journal: Option<Arc<dyn StepJournal>>,
    pub(crate) archive: ArchiveSettings,
    pub(crate) resume: Resumption,
    /// The read guard of a job an MCP call submitted (`read_guard.rs`); `None` otherwise.
    pub(crate) read_guard: Option<crate::read_guard::JobGuard>,
}

/// What a transfer needs to survive a lost connection and a restart (RC-060-07).
#[derive(Clone, Default)]
pub(crate) struct Resumption {
    pub(crate) retry: RetryPolicy,
    pub(crate) reconnect: Option<Arc<dyn Reconnect>>,
    pub(crate) store: Option<Arc<dyn TransferStore>>,
    /// The job's record in the store, once kept.
    pub(crate) record: Option<i64>,
    /// The file a restored job was writing when it stopped.
    pub(crate) mark: Option<FileMark>,
    /// Put back into the queue after a restart: files already complete at the target count as
    /// done.
    pub(crate) restored: bool,
}

pub(crate) struct Runner {
    pub(crate) job: Arc<JobShared>,
    pub(crate) options: JobOptions,
    pub(crate) buffer_size: usize,
    /// The stream pair's buffers, made once for all the job's files (RC-080-45).
    pub(crate) buffers: Buffers,
    pub(crate) src_id: BackendId,
    pub(crate) src: Arc<dyn Vfs>,
    pub(crate) trash: Option<Arc<dyn Trash>>,
    pub(crate) dst: Arc<dyn Vfs>,
    /// Where a rename job records its steps.
    pub(crate) journal: Option<Arc<dyn StepJournal>>,
    /// What the job may not read (`read_guard.rs`).
    pub(crate) read_guard: Option<crate::read_guard::JobGuard>,
    pub(crate) progress: Progress,
    /// Moving may still rename within one backend (until a rename crosses devices).
    pub(crate) rename_ok: bool,
    /// A move: each copied file's size is checked before its source goes.
    pub(crate) moving: bool,
    /// The target is a disk or a server: files are written under a part name (`staging.rs`).
    pub(crate) staged: bool,
    /// A copy or move to or from a server, or from disk to disk: its part files are kept and
    /// continued (`resume.rs`). Out of an archive a file is staged but starts over.
    pub(crate) resumable: bool,
    /// The target's backend, once a transfer knows it: the one a lost connection reopens.
    pub(crate) dst_id: Option<BackendId>,
    /// The target, when the write side of a file's stream failed: the failure names the side
    /// that failed (`stream_pair.rs`, RC-060-06). Cleared when a file's attempt starts, taken by
    /// [`Runner::failed`].
    pub(crate) failed_on: Option<VfsPath>,
    pub(crate) retry: RetryPolicy,
    pub(crate) reconnect: Option<Arc<dyn Reconnect>>,
    pub(crate) store: Option<Arc<dyn TransferStore>>,
    pub(crate) record: Option<i64>,
    pub(crate) mark: Option<FileMark>,
    pub(crate) restored: bool,
    /// How archives are read when unpacking.
    pub(crate) archive: ArchiveSettings,
    meter: Meter,
    throttle: Throttle,
    /// The progress moved since the snapshot last took it (RC-080-45).
    unpublished: bool,
    /// A conflict answer given "for all".
    remembered: Option<ConflictChoice>,
}

impl Runner {
    pub(crate) fn new(setup: Setup) -> Self {
        Self {
            job: setup.job,
            options: setup.options,
            buffer_size: setup.buffer_size.max(1),
            buffers: Buffers::default(),
            src_id: setup.src_id,
            src: setup.src,
            trash: setup.trash,
            dst: setup.dst,
            journal: setup.journal,
            read_guard: setup.read_guard,
            progress: Progress::default(),
            rename_ok: true,
            moving: false,
            staged: false,
            resumable: false,
            dst_id: None,
            failed_on: None,
            retry: setup.resume.retry,
            reconnect: setup.resume.reconnect,
            store: setup.resume.store,
            record: setup.resume.record,
            mark: setup.resume.mark,
            restored: setup.resume.restored,
            archive: setup.archive,
            meter: Meter::default(),
            throttle: Throttle::new(setup.progress_interval),
            unpublished: false,
            remembered: None,
        }
    }

    /// Does the job; the progress at its end comes with the outcome.
    pub(crate) async fn run(&mut self, kind: &JobKind) -> Result<(), Stop> {
        match kind {
            JobKind::Copy { sources, target } => self.transfer(sources, target, false).await,
            JobKind::Move { sources, target } => self.transfer(sources, target, true).await,
            JobKind::Delete { sources, mode } => self.delete(sources, *mode).await,
            JobKind::Mkdir { path } => self.mkdir(path).await,
            JobKind::Rename { dir, steps, .. } => self.rename(dir, steps).await,
            JobKind::CopyAs { source, target } => self.transfer_as(source, target, false).await,
            JobKind::MoveAs { source, target } if self.changes_case_only(source, target) => {
                self.rename_case(source, target).await
            }
            JobKind::MoveAs { source, target } => self.transfer_as(source, target, true).await,
            JobKind::TestArchive { archive } => self.test_archive(archive).await,
            JobKind::Extract {
                archives,
                target,
                settings,
            } => self.extract(archives, target, settings).await,
            JobKind::Pack {
                sources,
                target,
                settings,
            } => self.pack(sources, target, settings).await,
            JobKind::ChecksumCreate { .. }
            | JobKind::ChecksumVerify { .. }
            | JobKind::Split { .. }
            | JobKind::Combine { .. } => self.checksum(kind).await,
        }
    }

    /// `path` on the source backend.
    pub(crate) fn src_path(&self, path: &Path) -> VfsPath {
        VfsPath {
            backend: self.src_id.clone(),
            path: path.to_path_buf(),
        }
    }

    /// Waits while paused; stops when cancelled.
    pub(crate) async fn checkpoint(&mut self) -> Result<(), Stop> {
        let paused = self.job.control.is_paused();
        if paused {
            // A paused job's snapshot shows how far it got.
            self.publish();
        }
        self.job.control.checkpoint().await?;
        if paused {
            self.meter.reset();
        }
        Ok(())
    }

    /// Publishes the progress when the throttle (or `force`) lets it: into the snapshot and as
    /// an event. Between two, the snapshot keeps the last one - no lock and no copy per buffer
    /// (RC-080-45); a pause or a question publishes what is left ([`Runner::publish`]).
    pub(crate) fn tick(&mut self, force: bool) {
        let now = Instant::now();
        if !(self.throttle.ready(now) || force) {
            self.unpublished = true;
            return;
        }
        self.progress.measure(&mut self.meter, now);
        self.unpublished = false;
        let progress = self.progress.clone();
        self.job
            .update(|snapshot| snapshot.progress = progress.clone());
        self.job.emit(JobChange::Progress { progress });
    }

    /// The progress not yet published into the snapshot, before the job waits: on a pause or a
    /// question the snapshot shows where it stands. No event: the pause or the question is one.
    pub(crate) fn publish(&mut self) {
        if std::mem::take(&mut self.unpublished) {
            let progress = self.progress.clone();
            self.job.update(|snapshot| snapshot.progress = progress);
        }
    }

    /// Counts `files` files of `bytes` bytes as done (copied, skipped or failed).
    pub(crate) fn count_done(&mut self, files: u64, bytes: u64) {
        self.progress.files_done += files;
        self.progress.bytes_done += bytes;
        self.tick(false);
    }

    /// Counts a file a conflict answer skipped.
    pub(crate) fn count_skipped(&mut self, bytes: u64) {
        self.job.update(|snapshot| snapshot.skipped += 1);
        self.count_done(1, bytes);
    }

    /// A failing step on `path`: retry or skip (recorded as a failure), or stop.
    pub(crate) async fn failed(
        &mut self,
        path: &VfsPath,
        error: OpsError,
    ) -> Result<ErrorChoice, Stop> {
        let mut failure = Failure::new(path, &error);
        failure.target = self.failed_on.take().map(Box::new);
        let choice = match self.options.on_error {
            ErrorPolicy::Skip => ErrorChoice::Skip,
            ErrorPolicy::Cancel => ErrorChoice::Cancel,
            ErrorPolicy::Ask => match self.ask(Question::Error(failure.clone())).await? {
                Answer::Error { choice } => choice,
                // `Queue::answer` lets only fitting answers through.
                _ => ErrorChoice::Cancel,
            },
        };
        match choice {
            ErrorChoice::Retry => Ok(ErrorChoice::Retry),
            ErrorChoice::Skip => {
                self.job.update(|snapshot| snapshot.failures.push(failure));
                Ok(ErrorChoice::Skip)
            }
            ErrorChoice::Cancel => {
                // The operation stops: the other jobs of its batch too (RC-010-14).
                self.job.cancel_siblings();
                Err(Stop::Failed(failure))
            }
        }
    }

    /// Asks the caller and waits; a cancel while waiting stops the job.
    async fn ask(&mut self, question: Question) -> Result<Answer, Stop> {
        self.publish();
        let answer = self.job.ask(question.clone());
        self.job.emit(JobChange::Question { question });
        let answer = tokio::select! {
            answer = answer => answer.ok(),
            () = self.job.control.cancelled() => None,
        };
        self.job.settle();
        self.job.emit(JobChange::Answered);
        self.meter.reset();
        answer.ok_or(Stop::Cancelled)
    }

    /// The password of the encrypted `archive`, asked of the caller (`wrong`: the last one did
    /// not fit); `None` when the person skips it, or at once when the job may not ask (an
    /// `on_error` preset, as every MCP job has). A cancel while waiting stops the job.
    pub(crate) async fn ask_password(
        &mut self,
        archive: &VfsPath,
        wrong: bool,
    ) -> Result<Option<ArchivePassword>, Stop> {
        if self.options.on_error != ErrorPolicy::Ask {
            return Ok(None);
        }
        let question = Question::Password {
            archive: archive.clone(),
            wrong,
        };
        self.publish();
        let answer = self.job.ask_password(question.clone());
        self.job.emit(JobChange::Question { question });
        let answer = tokio::select! {
            answer = answer => answer.ok(),
            () = self.job.control.cancelled() => None,
        };
        self.job.settle();
        self.job.emit(JobChange::Answered);
        self.meter.reset();
        answer.ok_or(Stop::Cancelled)
    }

    /// What to do with a file whose target exists: the preset, the answer "for all" - this
    /// job's or a sibling's of its batch -, or the caller's answer. `resumable`: the target is a
    /// part file that can be continued, and "resume" does that. `Cancel` stops the batch.
    pub(crate) async fn conflict(
        &mut self,
        source: (&VfsPath, &rc_vfs::Entry),
        target: (&VfsPath, &rc_vfs::Entry),
        resumable: bool,
    ) -> Result<Decision, Stop> {
        let remembered = self.remembered.or_else(|| self.job.remembered());
        let choice = match self.options.on_conflict.or(remembered) {
            Some(choice) => choice,
            None => {
                let question = Question::Conflict {
                    source: EntryInfo::new(source.0, source.1),
                    target: EntryInfo::new(target.0, target.1),
                    resumable,
                };
                match self.ask(question).await? {
                    Answer::Conflict { choice, for_all } => {
                        if for_all {
                            self.remembered = Some(choice);
                            self.job.remember(choice);
                        }
                        choice
                    }
                    // Checked by `Queue::answer` (`crate::name::check_name`).
                    Answer::RenameTo { name } => return Ok(Decision::RenameTo(name)),
                    // `Compare` keeps the question open and never reaches the job.
                    Answer::Compare | Answer::Error { .. } => ConflictChoice::Cancel,
                }
            }
        };
        if choice == ConflictChoice::Cancel {
            self.job.cancel_siblings();
        }
        if choice == ConflictChoice::Resume && resumable {
            return Ok(Decision::Resume);
        }
        Ok(decide(choice, source.1, target.1))
    }
}
