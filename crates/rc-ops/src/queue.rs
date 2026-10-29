//! The queue: accepts jobs, runs them in order per lane (target backend, or a server's
//! connection), and lets the caller steer them. Transfers to and from servers are kept by a
//! store and put back after a restart (`restore.rs`, RC-060-07). Every job belongs to one of the
//! queue's named lists, which start and stop on their own (`lists.rs`, RC-010-15). The jobs of one
//! operation of several sources are submitted as a batch, one job per source (`batch.rs`).

mod batch;
mod bulk;
mod config;
mod lanes;
mod lists;
mod order;
mod placement;
mod restore;
mod setup;
mod shared;
mod task;

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use rc_vfs::{ArchivePassword, BackendId, VfsError};
use tokio::sync::oneshot;

pub use batch::{BatchId, per_item};
pub use config::{
    Backend, DEFAULT_BUFFER_SIZE, DEFAULT_MAX_RUNNING, DEFAULT_PER_CONNECTION, QueueConfig,
    Scheduling,
};
pub use lanes::Reorder;
use lanes::{Lanes, Turn};
pub use lists::{AT_ONCE, DEFAULT_LIST, MAX_LIST_NAME, QueueList, QueueListId, check_list_name};
pub use order::QueueEntry;
pub(crate) use placement::fold;
use placement::{check_placement, check_placement_as, one_backend};
pub use restore::is_saveable;
pub(crate) use shared::JobShared;
use task::run;

use crate::{
    Answer, DeleteMode, EventSink, JobChange, JobId, JobKind, JobSnapshot, JobSpec, OpsError,
    journal::StepJournal, persist::TransferStore, resume::Reconnect, runner::Resumption,
};

/// The job queue. Clones share it. Must be used inside a Tokio runtime: every job is a task.
#[derive(Clone)]
pub struct Queue {
    inner: Arc<Inner>,
}

struct Inner {
    config: Mutex<QueueConfig>,
    backends: Mutex<HashMap<BackendId, Backend>>,
    jobs: Mutex<BTreeMap<JobId, Arc<JobShared>>>,
    lists: Mutex<lists::ListBoard>,
    lanes: Arc<Lanes>,
    next: AtomicU64,
    sink: EventSink,
    hooks: Mutex<Hooks>,
}

/// What the queue is given to keep transfers and reopen servers (RC-060-07).
#[derive(Clone, Default)]
struct Hooks {
    store: Option<Arc<dyn TransferStore>>,
    reconnect: Option<Arc<dyn Reconnect>>,
    /// The read guard of the jobs an MCP call submits (`read_guard.rs`, RC-080-42).
    read_guards: Option<Arc<dyn crate::ReadGuards>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Queue {
    /// An empty queue with no backends, reporting to `sink`.
    #[must_use]
    pub fn new(config: QueueConfig, sink: EventSink) -> Self {
        let lanes = Arc::new(Lanes::new(config.limits()));
        Self {
            inner: Arc::new(Inner {
                config: Mutex::new(config),
                backends: Mutex::new(HashMap::new()),
                jobs: Mutex::new(BTreeMap::new()),
                lists: Mutex::new(lists::ListBoard::default()),
                lanes,
                next: AtomicU64::new(1),
                sink,
                hooks: Mutex::new(Hooks::default()),
            }),
        }
    }

    /// Makes `backend` the file system of `id`, for jobs submitted from now on.
    pub fn register(&self, id: BackendId, backend: Backend) {
        lock(&self.inner.backends).insert(id, backend);
    }

    /// Whether the file system of `id` is registered with a recycle bin.
    #[must_use]
    pub fn has_trash(&self, id: &BackendId) -> bool {
        lock(&self.inner.backends)
            .get(id)
            .is_some_and(|backend| backend.trash.is_some())
    }

    /// Forgets the file system of `id`; running jobs keep theirs.
    pub fn unregister(&self, id: &BackendId) {
        lock(&self.inner.backends).remove(id);
    }

    /// The settings.
    #[must_use]
    pub fn config(&self) -> QueueConfig {
        lock(&self.inner.config).clone()
    }

    /// Changes when jobs submitted from now on run.
    pub fn set_scheduling(&self, scheduling: Scheduling) {
        lock(&self.inner.config).scheduling = scheduling;
    }

    /// Changes the buffer of the jobs submitted from now on and how many jobs run at once; waiting
    /// jobs start at once where the new limits leave room. Each value is at least one.
    pub fn set_transfer_limits(
        &self,
        buffer_size: usize,
        per_connection: usize,
        max_running: usize,
    ) {
        let limits = {
            let mut config = lock(&self.inner.config);
            config.buffer_size = buffer_size.max(1);
            config.per_connection = per_connection.max(1);
            config.max_running = max_running.max(1);
            config.limits()
        };
        self.inner.lanes.set_limits(limits);
    }

    fn backend(&self, id: &BackendId) -> Result<Backend, OpsError> {
        lock(&self.inner.backends)
            .get(id)
            .cloned()
            .ok_or(OpsError::BackendUnavailable)
    }

    /// Accepts a job and starts it when its turn comes; its snapshot as queued.
    ///
    /// # Errors
    ///
    /// The job cannot work: no sources, sources on several backends, a backend not
    /// registered, a directory into itself or an entry onto itself, a recycle bin the
    /// backend does not have.
    pub fn submit(&self, spec: JobSpec) -> Result<JobSnapshot, OpsError> {
        self.submit_journaled(spec, None)
    }

    /// [`Queue::submit`], with a journal a rename job records each step in
    /// ([`StepJournal`]); other jobs ignore it.
    ///
    /// # Errors
    ///
    /// As [`Queue::submit`].
    pub fn submit_journaled(
        &self,
        spec: JobSpec,
        journal: Option<Arc<dyn StepJournal>>,
    ) -> Result<JobSnapshot, OpsError> {
        self.submit_into(spec, journal, DEFAULT_LIST)
    }

    /// [`Queue::submit_journaled`] into the list `list`: a stopped list holds the job until it
    /// starts (RC-010-15).
    ///
    /// # Errors
    ///
    /// As [`Queue::submit`]; `queue_list_not_found`.
    pub fn submit_into(
        &self,
        spec: JobSpec,
        journal: Option<Arc<dyn StepJournal>>,
        list: QueueListId,
    ) -> Result<JobSnapshot, OpsError> {
        let spec = self.marked(spec);
        let (setup_src, setup_dst) = self.check(&spec.kind)?;
        let job = self.accept(&spec, list, None)?;
        let snapshot = job.snapshot();
        let lane = self.enter_lane(&job, &spec.kind);
        let resume = Resumption {
            store: self.hooks().store,
            ..Resumption::default()
        };
        let setup = self.setup(&job, spec.options, (setup_src, setup_dst), journal, resume);
        tokio::spawn(run(job, spec.kind, lane, setup));
        Ok(snapshot)
    }

    /// A new job of `spec` in the list `list` - held while the list is stopped -, one of `batch`,
    /// listed and announced as queued.
    fn accept(
        &self,
        spec: &JobSpec,
        list: QueueListId,
        batch: Option<&Arc<batch::Batch>>,
    ) -> Result<Arc<JobShared>, OpsError> {
        let keep = self.config().keep_finished;
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let sink = Arc::clone(&self.inner.sink);
        let job = Arc::new(JobShared::new(id, spec, sink, batch));
        {
            // The list's lock first: a list that starts meanwhile finds the job (`lists.rs`).
            let lists = lock(&self.inner.lists);
            job.set_list(list);
            job.control.hold(!lists.started(list)?);
            let mut jobs = lock(&self.inner.jobs);
            jobs.insert(id, Arc::clone(&job));
            forget_finished(&mut jobs, keep);
        }
        job.emit(JobChange::Queued);
        Ok(job)
    }

    /// The job's place in its lane, when jobs wait for one.
    fn enter_lane(
        &self,
        job: &Arc<JobShared>,
        kind: &JobKind,
    ) -> Option<(Turn, oneshot::Receiver<()>)> {
        match self.config().scheduling {
            _ if job.list() == AT_ONCE => None,
            Scheduling::Parallel => None,
            Scheduling::PerTargetBackend => kind
                .lane()
                .map(|path| self.inner.lanes.enter(&path.backend, Arc::clone(job))),
        }
    }

    /// The source backend (with its id) and the target backend of a job that can work.
    fn check(&self, kind: &JobKind) -> Result<((BackendId, Backend), Backend), OpsError> {
        match kind {
            JobKind::Move { sources, .. }
                if sources
                    .first()
                    .is_some_and(|source| matches!(source.backend, BackendId::Archive { .. })) =>
            {
                // An archive is read-only: F6 out of it is refused (RC-030-03), F5 copies.
                Err(OpsError::Vfs(VfsError::Unsupported {
                    path: sources[0].path.clone(),
                    operation: "moving out of an archive",
                }))
            }
            JobKind::Copy { sources, target } | JobKind::Move { sources, target } => {
                let source_id = one_backend(sources)?;
                let source = self.backend(&source_id)?;
                let destination = self.backend(&target.backend)?;
                if source_id == target.backend {
                    let case_sensitive = source.vfs.capabilities().case_sensitive;
                    for source in sources {
                        check_placement(&source.path, &target.path, case_sensitive)?;
                    }
                }
                Ok(((source_id, source), destination))
            }
            JobKind::Delete { sources, mode } => {
                let source_id = one_backend(sources)?;
                let source = self.backend(&source_id)?;
                if *mode == DeleteMode::Trash && source.trash.is_none() {
                    return Err(OpsError::TrashUnavailable {
                        path: sources[0].path.clone(),
                    });
                }
                Ok(((source_id, source.clone()), source))
            }
            JobKind::Rename { dir, steps, .. } => {
                crate::rename::check_steps(steps)?;
                let backend = self.backend(&dir.backend)?;
                Ok(((dir.backend.clone(), backend.clone()), backend))
            }
            JobKind::Mkdir { path: dir }
            | JobKind::TestArchive { archive: dir }
            | JobKind::ChecksumCreate { dir, .. }
            | JobKind::ChecksumVerify { file: dir } => {
                let backend = self.backend(&dir.backend)?;
                Ok(((dir.backend.clone(), backend.clone()), backend))
            }
            JobKind::CopyAs { source, target } | JobKind::MoveAs { source, target } => {
                let name = target
                    .path
                    .file_name()
                    .filter(|_| target.path.parent().is_some())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                crate::check_name(&name)?;
                let source_backend = self.backend(&source.backend)?;
                let destination = self.backend(&target.backend)?;
                if source.backend == target.backend {
                    let case_sensitive = source_backend.vfs.capabilities().case_sensitive;
                    let moving = matches!(kind, JobKind::MoveAs { .. });
                    check_placement_as(&source.path, &target.path, case_sensitive, moving)?;
                }
                Ok(((source.backend.clone(), source_backend), destination))
            }
            JobKind::Extract {
                archives: sources,
                target,
                ..
            } => {
                let source_id = one_backend(sources)?;
                let source = self.backend(&source_id)?;
                Ok(((source_id, source), self.backend(&target.backend)?))
            }
            JobKind::Pack {
                sources, target, ..
            } => {
                let source_id = one_backend(sources)?;
                let source = self.backend(&source_id)?;
                // The archive writers need a local file (a 7z is written with seeks).
                if target.backend != BackendId::Local {
                    return Err(OpsError::Vfs(VfsError::Unsupported {
                        path: target.path.clone(),
                        operation: "packing onto this file system",
                    }));
                }
                Ok(((source_id, source), self.backend(&target.backend)?))
            }
            JobKind::Split { source, target, .. }
            | JobKind::Combine {
                first: source,
                target,
                ..
            } => {
                let source_backend = self.backend(&source.backend)?;
                Ok((
                    (source.backend.clone(), source_backend),
                    self.backend(&target.backend)?,
                ))
            }
        }
    }

    fn shared(&self, id: JobId) -> Result<Arc<JobShared>, OpsError> {
        lock(&self.inner.jobs)
            .get(&id)
            .cloned()
            .ok_or(OpsError::JobNotFound { job: id })
    }

    /// Every job the queue knows, oldest first: running, queued and recently finished.
    #[must_use]
    pub fn jobs(&self) -> Vec<JobSnapshot> {
        let jobs: Vec<Arc<JobShared>> = lock(&self.inner.jobs).values().cloned().collect();
        jobs.iter().map(|job| job.snapshot()).collect()
    }

    /// One job.
    ///
    /// # Errors
    ///
    /// `job_not_found`.
    pub fn job(&self, id: JobId) -> Result<JobSnapshot, OpsError> {
        Ok(self.shared(id)?.snapshot())
    }

    /// Pauses a job at its next buffer; a queued job keeps its place but lets the jobs behind it
    /// start first.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_finished`.
    pub fn pause(&self, id: JobId) -> Result<JobSnapshot, OpsError> {
        let job = self.running(id)?;
        if job.control.pause() {
            job.emit(JobChange::Paused);
        }
        Ok(job.snapshot())
    }

    /// Resumes a paused job.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_finished`.
    pub fn resume(&self, id: JobId) -> Result<JobSnapshot, OpsError> {
        let job = self.running(id)?;
        if job.control.resume() {
            job.emit(JobChange::Resumed);
        }
        self.inner.lanes.kick();
        job.kick_batch();
        Ok(job.snapshot())
    }

    /// Cancels a job, also a paused or a waiting one. The file being written is removed; what
    /// is complete stays. The job's end comes as its `finished` event.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_finished`.
    pub fn cancel(&self, id: JobId) -> Result<JobSnapshot, OpsError> {
        let job = self.running(id)?;
        job.control.cancel();
        Ok(job.snapshot())
    }

    /// Answers the question a job waits on.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_finished`, `no_question` when the job asks nothing or something
    /// else.
    pub fn answer(&self, id: JobId, answer: Answer) -> Result<JobSnapshot, OpsError> {
        let job = self.running(id)?;
        job.answer(answer)?;
        Ok(job.snapshot())
    }

    /// Answers a job's password question (an encrypted archive to unpack): the password, or
    /// `None` to skip the archive's encrypted entries. The password lives for the job only.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_finished`, `no_question` when the job asks no password.
    pub fn answer_password(
        &self,
        id: JobId,
        password: Option<ArchivePassword>,
    ) -> Result<JobSnapshot, OpsError> {
        let job = self.running(id)?;
        job.answer_password(password)?;
        Ok(job.snapshot())
    }

    /// Waits until a job has finished; its last snapshot.
    ///
    /// # Errors
    ///
    /// `job_not_found`.
    pub async fn wait(&self, id: JobId) -> Result<JobSnapshot, OpsError> {
        let job = self.shared(id)?;
        job.finished().await;
        Ok(job.snapshot())
    }

    fn running(&self, id: JobId) -> Result<Arc<JobShared>, OpsError> {
        let job = self.shared(id)?;
        if job.snapshot().state.is_finished() {
            return Err(OpsError::JobFinished { job: id });
        }
        Ok(job)
    }
}

/// Drops the oldest finished jobs beyond `keep`.
fn forget_finished(jobs: &mut BTreeMap<JobId, Arc<JobShared>>, keep: usize) {
    let finished: Vec<JobId> = jobs
        .iter()
        .filter(|(_, job)| job.snapshot().state.is_finished())
        .map(|(id, _)| *id)
        .collect();
    for id in finished.iter().take(finished.len().saturating_sub(keep)) {
        jobs.remove(id);
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
