//! A job as both sides hold it: the snapshot, the flags, the open question, its list.

use std::sync::{
    Arc, Mutex, MutexGuard, PoisonError,
    atomic::{AtomicU64, Ordering},
};

use rc_vfs::ArchivePassword;
use tokio::sync::{oneshot, watch};

use super::batch::Batch;
use crate::{
    Answer, EventSink, JobChange, JobEvent, JobId, JobSnapshot, JobSpec, JobState, OpsError,
    Progress, Question, check_name, control::Control, runner::Stop,
};

pub(crate) struct JobShared {
    pub(crate) id: JobId,
    pub(crate) control: Control,
    /// The list it belongs to ([`crate::QueueListId`], RC-010-15).
    list: AtomicU64,
    /// The other jobs of its operation, when it is one of several (`batch.rs`, RC-010-14).
    pub(crate) batch: Option<Arc<Batch>>,
    state: Mutex<Held>,
    done: watch::Sender<bool>,
    sink: EventSink,
}

/// Snapshot and question under one lock, so an answer never finds one without the other.
struct Held {
    snapshot: JobSnapshot,
    pending: Option<oneshot::Sender<Answer>>,
    /// The open password question of an extraction; `None` answers "skip this archive".
    password: Option<oneshot::Sender<Option<ArchivePassword>>>,
}

impl JobShared {
    pub(crate) fn new(
        id: JobId,
        spec: &JobSpec,
        sink: EventSink,
        batch: Option<&Arc<Batch>>,
    ) -> Self {
        Self {
            id,
            control: Control::default(),
            list: AtomicU64::new(crate::DEFAULT_LIST),
            batch: batch.cloned(),
            state: Mutex::new(Held {
                snapshot: JobSnapshot {
                    id,
                    kind: spec.kind.clone(),
                    options: spec.options,
                    state: JobState::Queued,
                    progress: Progress::default(),
                    question: None,
                    skipped: 0,
                    failures: Vec::new(),
                    report: None,
                    resumable: false,
                    batch: batch.map(|batch| batch.id),
                },
                pending: None,
                password: None,
            }),
            done: watch::Sender::new(false),
            sink,
        }
    }

    pub(crate) fn list(&self) -> crate::QueueListId {
        self.list.load(Ordering::Relaxed)
    }

    pub(crate) fn set_list(&self, list: crate::QueueListId) {
        self.list.store(list, Ordering::Relaxed);
    }

    fn held(&self) -> MutexGuard<'_, Held> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The job as callers see it: a paused job that is not waiting on a question reads
    /// `paused`.
    pub(crate) fn snapshot(&self) -> JobSnapshot {
        let mut snapshot = self.held().snapshot.clone();
        snapshot.state = self.shown(snapshot.state);
        snapshot
    }

    /// The state and the progress of [`JobShared::snapshot`], without a copy of the rest - the
    /// sources, the failures, the report (RC-080-45).
    pub(crate) fn progress(&self) -> (JobState, Progress) {
        let (state, progress) = {
            let held = self.held();
            (held.snapshot.state, held.snapshot.progress.clone())
        };
        (self.shown(state), progress)
    }

    /// `state` as callers see it: a busy job that is paused reads `paused`.
    fn shown(&self, state: JobState) -> JobState {
        let busy = matches!(
            state,
            JobState::Queued | JobState::Counting | JobState::Running
        );
        if busy && self.control.is_paused() {
            JobState::Paused
        } else {
            state
        }
    }

    pub(crate) fn update<R>(&self, change: impl FnOnce(&mut JobSnapshot) -> R) -> R {
        change(&mut self.held().snapshot)
    }

    /// Sends an event; never under the lock, so the sink may call back into the queue.
    pub(crate) fn emit(&self, change: JobChange) {
        (self.sink)(JobEvent {
            job: self.id,
            change,
        });
    }

    /// Puts up `question`; the receiver gets the answer.
    pub(crate) fn ask(&self, question: Question) -> oneshot::Receiver<Answer> {
        let (sender, receiver) = oneshot::channel();
        let mut held = self.held();
        held.snapshot.question = Some(question);
        held.snapshot.state = JobState::Waiting;
        held.pending = Some(sender);
        receiver
    }

    /// Puts up the password question `question`; the receiver gets the password, or `None`.
    pub(crate) fn ask_password(
        &self,
        question: Question,
    ) -> oneshot::Receiver<Option<ArchivePassword>> {
        let (sender, receiver) = oneshot::channel();
        let mut held = self.held();
        held.snapshot.question = Some(question);
        held.snapshot.state = JobState::Waiting;
        held.password = Some(sender);
        receiver
    }

    /// Hands a typed password (or `None`, skip the archive) to the job waiting for one.
    pub(crate) fn answer_password(
        &self,
        password: Option<ArchivePassword>,
    ) -> Result<(), OpsError> {
        let refused = OpsError::NoQuestion { job: self.id };
        let mut held = self.held();
        if !matches!(held.snapshot.question, Some(Question::Password { .. })) {
            return Err(refused);
        }
        let sender = held.password.take().ok_or(refused)?;
        sender
            .send(password)
            .map_err(|_| OpsError::NoQuestion { job: self.id })
    }

    /// Takes the question down after it was answered or the job was cancelled.
    pub(crate) fn settle(&self) {
        let mut held = self.held();
        held.snapshot.question = None;
        held.pending = None;
        held.password = None;
        if held.snapshot.state == JobState::Waiting {
            held.snapshot.state = JobState::Running;
        }
    }

    /// Hands `answer` to the waiting job.
    /// A typed name is checked first: a bad one is refused and the question stays open, as it
    /// does for `Compare`.
    pub(crate) fn answer(&self, answer: Answer) -> Result<(), OpsError> {
        let mut held = self.held();
        let refused = OpsError::NoQuestion { job: self.id };
        let fits = held
            .snapshot
            .question
            .as_ref()
            .is_some_and(|question| answer.fits(question));
        if !fits {
            return Err(refused);
        }
        match &answer {
            Answer::RenameTo { name } => check_name(name)?,
            Answer::Compare if held.pending.is_some() => return Ok(()),
            _ => {}
        }
        let Some(sender) = held.pending.take() else {
            return Err(refused);
        };
        sender.send(answer).map_err(|_| refused)
    }

    /// Records the end, announces it and wakes [`crate::Queue::wait`].
    pub(crate) fn finish(&self, outcome: Result<(), Stop>, progress: Option<Progress>) {
        let state = match &outcome {
            Ok(()) => JobState::Completed,
            Err(Stop::Cancelled) => JobState::Cancelled,
            Err(Stop::Failed(_)) => JobState::Failed,
        };
        let progress = {
            let mut held = self.held();
            let snapshot = &mut held.snapshot;
            snapshot.state = state;
            snapshot.question = None;
            if let Some(progress) = progress {
                snapshot.progress = progress;
            }
            snapshot.progress.current = None;
            snapshot.progress.file_bytes_done = 0;
            snapshot.progress.file_bytes_total = 0;
            snapshot.progress.bytes_per_second = 0;
            snapshot.progress.eta_seconds = None;
            if let Err(Stop::Failed(failure)) = outcome {
                snapshot.failures.push(failure);
            }
            held.pending = None;
            held.password = None;
            held.snapshot.progress.clone()
        };
        self.emit(JobChange::Progress { progress });
        self.emit(JobChange::Finished { state });
        self.done.send_replace(true);
    }

    /// Resolves once the job has finished.
    pub(crate) async fn finished(&self) {
        let mut done = self.done.subscribe();
        let _ = done.wait_for(|done| *done).await;
    }
}
