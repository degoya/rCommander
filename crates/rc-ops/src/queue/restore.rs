//! Transfers across a restart (RC-060-07): the store that keeps them, the hook that reopens a
//! server, the retry setting, and the jobs put back into the queue after a restart.
//!
//! A restored job is listed at once, paused and `resumable`, in its lane like any job, but its
//! backends are looked up only when it is resumed: a server's connection is not open when the app
//! starts, so the [`Reconnect`] hook opens it then. A server that cannot be reached leaves the job
//! paused with the error among its failures; resuming tries again. Cancelling a restored job
//! forgets its record and removes the part file it left, where its backend is open.
//!
//! A job of a saved list that is loaded again (RC-010-15) takes the same way, without the pause:
//! it waits in its list, held until the list starts, and opens its servers when its turn comes.

use std::sync::Arc;

use rc_vfs::VfsError;

use super::{
    Backend, Hooks, JobShared, Queue,
    batch::Batch,
    lock,
    task::{take_turn, work},
};
use crate::{
    Failure, JobChange, JobKind, JobSnapshot, JobSpec, OpsError,
    persist::{FileMark, SavedJob, TransferStore, is_kept},
    resume::{Reconnect, RetryPolicy},
    runner::{Resumption, Setup, Stop},
    staging::{is_remote, remove_leftover},
    temp_name::part_path,
};

impl Queue {
    /// Keeps the transfers to and from servers in `store` from the next job on.
    pub fn set_store(&self, store: Arc<dyn TransferStore>) {
        lock(&self.inner.hooks).store = Some(store);
    }

    /// Lets transfers reopen a server through `hook` after a lost connection.
    pub fn set_reconnect(&self, hook: Arc<dyn Reconnect>) {
        lock(&self.inner.hooks).reconnect = Some(hook);
    }

    /// How the transfers started from now on meet a lost connection.
    pub fn set_retry(&self, retry: RetryPolicy) {
        lock(&self.inner.config).retry = retry;
    }

    /// The archive limits the unpack and test jobs started from now on read with (RC-080-35).
    pub fn set_archive_limits(&self, limits: rc_vfs::ArchiveLimits) {
        lock(&self.inner.config).archive.limits = limits;
    }

    /// Puts a job the store kept back into the queue, paused and resumable: `ops.resume` (or
    /// [`Queue::resume`]) continues it, a cancel discards it.
    ///
    /// # Errors
    ///
    /// `unsupported` for a job that is no transfer to or from a server.
    pub fn restore(&self, saved: SavedJob) -> Result<JobSnapshot, OpsError> {
        self.restore_in(saved, None)
    }

    /// [`Queue::restore`] as one job of `batch` ([`Queue::restore_batch`]).
    pub(super) fn restore_in(
        &self,
        saved: SavedJob,
        batch: Option<&Arc<Batch>>,
    ) -> Result<JobSnapshot, OpsError> {
        let SavedJob { record, spec, mark } = saved;
        if !is_kept(&spec.kind) {
            let path = spec
                .kind
                .lane()
                .map(|lane| lane.path.clone())
                .unwrap_or_default();
            return Err(OpsError::Vfs(VfsError::Unsupported {
                path,
                operation: "resuming this kind of job",
            }));
        }
        let job = self.accept(&spec, super::DEFAULT_LIST, batch)?;
        if let Some(batch) = batch {
            batch.join(&job);
        }
        job.control.pause();
        job.update(|snapshot| snapshot.resumable = true);
        job.emit(JobChange::Paused);
        let resume = Resumption {
            store: self.hooks().store,
            record: Some(record),
            mark,
            restored: true,
            ..Resumption::default()
        };
        Ok(self.adopt(job, spec, resume))
    }

    /// Puts a job of a saved list into the list `list`: it waits like any job - held while the
    /// list is stopped - and its servers are opened when its turn comes, through the
    /// [`Reconnect`] hook. A transfer is kept by the store from its start, like a submitted one.
    ///
    /// # Errors
    ///
    /// `queue_list_not_found`; `unsupported` for a job no saved list holds ([`is_saveable`]).
    pub fn load(&self, spec: JobSpec, list: super::QueueListId) -> Result<JobSnapshot, OpsError> {
        // Guarded when loaded inside an MCP call, whatever the saved list says (RC-080-42).
        let spec = self.marked(spec);
        if !is_saveable(&spec.kind) {
            return Err(OpsError::Vfs(VfsError::Unsupported {
                path: first_location(&spec.kind).path,
                operation: "loading this kind of job",
            }));
        }
        let job = self.accept(&spec, list, None)?;
        let resume = Resumption {
            store: self.hooks().store,
            ..Resumption::default()
        };
        Ok(self.adopt(job, spec, resume))
    }

    /// The task of a restored or loaded job, whose backends are looked up when its turn comes;
    /// its snapshot as accepted.
    fn adopt(&self, job: Arc<JobShared>, spec: JobSpec, resume: Resumption) -> JobSnapshot {
        let snapshot = job.snapshot();
        let lane = self.enter_lane(&job, &spec.kind);
        let queue = self.clone();
        tokio::spawn(async move {
            let Some(turn) = take_turn(&job, lane).await else {
                queue.discard_restored(&job, &spec, resume).await;
                return;
            };
            loop {
                if job.control.checkpoint().await.is_err() {
                    queue.discard_restored(&job, &spec, resume).await;
                    return;
                }
                match queue.restored_setup(&job, &spec, resume.clone()).await {
                    Ok(setup) => {
                        job.update(|snapshot| snapshot.resumable = false);
                        work(job, spec.kind, turn, setup).await;
                        return;
                    }
                    Err(error) => {
                        // The server cannot be had now: paused again, the reason listed.
                        let failure = Failure::new(&first_location(&spec.kind), &error);
                        job.update(|snapshot| snapshot.failures.push(failure));
                        if job.control.pause() {
                            job.emit(JobChange::Paused);
                        }
                    }
                }
            }
        });
        snapshot
    }

    /// The setup of a restored job being resumed: its servers opened again where they are not
    /// registered, then checked like any submitted job.
    async fn restored_setup(
        &self,
        job: &Arc<JobShared>,
        spec: &JobSpec,
        resume: Resumption,
    ) -> Result<Setup, OpsError> {
        let Hooks { reconnect, .. } = self.hooks();
        for backend in backends_of(&spec.kind) {
            if is_remote(backend)
                && self.backend(backend).is_err()
                && let Some(hook) = &reconnect
            {
                let vfs = hook.reconnect(backend).await?;
                self.register(backend.clone(), Backend::new(vfs));
            }
        }
        let backends = self.check(&spec.kind)?;
        Ok(self.setup(job, spec.options, backends, None, resume))
    }

    /// Ends a restored job that is cancelled before it ran: the record goes, and the part of the
    /// file it was writing where its backend is open.
    async fn discard_restored(&self, job: &JobShared, spec: &JobSpec, resume: Resumption) {
        if let Some(FileMark { target, .. }) = &resume.mark
            && let Some(backend) = target_of(&spec.kind)
            && let Ok(backend) = self.backend(backend)
        {
            remove_leftover(backend.vfs.as_ref(), &part_path(target)).await;
        }
        if let (Some(store), Some(record)) = (resume.store, resume.record) {
            store.forget(record).await;
        }
        job.finish(Err(Stop::Cancelled), None);
    }
}

/// Whether a job of `kind` can be saved in a list and loaded again (RC-010-15): a copy or move,
/// whose sources, target and options say all there is to it. A rename's steps, an archive's
/// settings and the other jobs are made anew where they are needed.
#[must_use]
pub fn is_saveable(kind: &JobKind) -> bool {
    matches!(
        kind,
        JobKind::Copy { .. }
            | JobKind::Move { .. }
            | JobKind::CopyAs { .. }
            | JobKind::MoveAs { .. }
    )
}

/// The backends a transfer reads and writes.
fn backends_of(kind: &JobKind) -> Vec<&rc_vfs::BackendId> {
    match kind {
        JobKind::Copy { sources, target } | JobKind::Move { sources, target } => sources
            .first()
            .map(|source| &source.backend)
            .into_iter()
            .chain([&target.backend])
            .collect(),
        JobKind::CopyAs { source, target } | JobKind::MoveAs { source, target } => {
            vec![&source.backend, &target.backend]
        }
        _ => Vec::new(),
    }
}

/// The backend a transfer writes to.
fn target_of(kind: &JobKind) -> Option<&rc_vfs::BackendId> {
    match kind {
        JobKind::Copy { target, .. }
        | JobKind::Move { target, .. }
        | JobKind::CopyAs { target, .. }
        | JobKind::MoveAs { target, .. } => Some(&target.backend),
        _ => None,
    }
}

/// Where a failure to set a restored job up is shown: its target.
fn first_location(kind: &JobKind) -> rc_vfs::VfsPath {
    match kind {
        JobKind::Copy { target, .. }
        | JobKind::Move { target, .. }
        | JobKind::CopyAs { target, .. }
        | JobKind::MoveAs { target, .. } => target.clone(),
        _ => rc_vfs::VfsPath::local(""),
    }
}
