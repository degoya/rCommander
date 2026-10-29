//! One operation as several jobs (RC-010-14): a copy, move or delete of several sources becomes
//! one job per source ([`per_item`]), submitted together ([`Queue::submit_batch`]) so they stand
//! side by side in their lane and start in the order they were selected - each with its own
//! progress, pause, place and cancel.
//!
//! Total Commander answers a conflict once for the whole operation, and so do the jobs of a
//! batch: an answer "for all" holds for every later conflict of each of them and answers the
//! conflict a sibling asks right now; `Cancel` answered to a conflict or a failing file - or given
//! in advance, `on_conflict: cancel`, `on_error: cancel` - stops the siblings that have not
//! finished too, as it stopped the one job before. Cancelling one job of a batch
//! ([`Queue::cancel`], the queue window) stops only that one.
//!
//! Submitted [`crate::AT_ONCE`] - confirmed with OK, not queued with F2 - the jobs of a batch run
//! one after the other without waiting for the queue's lanes, and are in none of the queue's
//! lists. They wait in a lane of their own with one place ([`Batch::turns`]): a waiting job that
//! is cancelled ends at once, a paused one lets the ones behind it go first, as in the queue.
//!
//! A batch outlives a restart: the store keeps its jobs one after the other, in their order, each
//! with its batch ([`crate::TransferStore::save_in`]), and [`Queue::restore_batch`] puts them back
//! as siblings. A saved list (RC-010-15) holds each job on its own; loaded, they are no batch.

use std::sync::{
    Arc, Mutex, MutexGuard, PoisonError, Weak,
    atomic::{AtomicU64, Ordering},
};

use rc_vfs::BackendId;

use super::{
    AT_ONCE, JobShared, Queue, QueueListId,
    lanes::{Lanes, Limits},
    task,
};
use crate::{
    Answer, ConflictChoice, JobId, JobKind, JobSnapshot, JobSpec, OpsError, Question, SavedJob,
    runner::Resumption,
};

/// The number of a batch, unique for the life of the process.
pub type BatchId = u64;

static NEXT: AtomicU64 = AtomicU64::new(1);

/// The jobs of one operation.
pub(crate) struct Batch {
    pub(crate) id: BatchId,
    state: Mutex<Siblings>,
    /// The lane of a batch run at once: one job at a time, in their order.
    turns: Option<Arc<Lanes>>,
}

struct Siblings {
    /// An answer "for all" one of them was given.
    remembered: Option<ConflictChoice>,
    /// Weak: a job holds its batch, not the other way round.
    members: Vec<Weak<JobShared>>,
}

/// The one place of a batch's own lane.
const ONE_AT_A_TIME: Limits = Limits {
    per_connection: 1,
    total: 1,
};

impl Batch {
    /// A batch; `at_once`, with a lane of its own.
    fn new(at_once: bool) -> Arc<Self> {
        Arc::new(Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            state: Mutex::new(Siblings {
                remembered: None,
                members: Vec::new(),
            }),
            turns: at_once.then(|| Arc::new(Lanes::new(ONE_AT_A_TIME))),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Siblings> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn join(&self, job: &Arc<JobShared>) {
        self.lock().members.push(Arc::downgrade(job));
    }

    /// The jobs of the batch but `job`, as long as they are there.
    fn others(&self, job: JobId) -> Vec<Arc<JobShared>> {
        self.lock()
            .members
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|member| member.id != job)
            .collect()
    }
}

impl JobShared {
    /// The answer "for all" a sibling was given.
    pub(crate) fn remembered(&self) -> Option<ConflictChoice> {
        self.batch.as_ref()?.lock().remembered
    }

    /// `choice` was answered "for all": it holds for the siblings' later conflicts, and answers
    /// the conflict one of them asks now.
    pub(crate) fn remember(&self, choice: ConflictChoice) {
        let Some(batch) = &self.batch else {
            return;
        };
        batch.lock().remembered = Some(choice);
        for sibling in batch.others(self.id) {
            if matches!(sibling.snapshot().question, Some(Question::Conflict { .. })) {
                // A sibling that was answered meanwhile has no question left: nothing to do.
                let _ = sibling.answer(Answer::Conflict {
                    choice,
                    for_all: true,
                });
            }
        }
    }

    /// A job of a batch run at once was resumed: the batch's lane starts it when it is free.
    pub(super) fn kick_batch(&self) {
        if let Some(turns) = self.batch.as_ref().and_then(|batch| batch.turns.as_ref()) {
            turns.kick();
        }
    }

    /// The operation was cancelled from this job: its siblings stop too, waiting or running.
    pub(crate) fn cancel_siblings(&self) {
        if let Some(batch) = &self.batch {
            for sibling in batch.others(self.id) {
                sibling.control.cancel();
            }
        }
    }
}

/// The jobs of `spec` one per item: a copy, move or delete of several sources becomes one job
/// per source, in their order, each with the options of the whole. Every other job stays one -
/// an archive packed from several sources is one item, the archive.
#[must_use]
pub fn per_item(spec: JobSpec) -> Vec<JobSpec> {
    let JobSpec { kind, options } = spec;
    let one = |kind| JobSpec { kind, options };
    match kind {
        JobKind::Copy { sources, target } if sources.len() > 1 => sources
            .into_iter()
            .map(|source| {
                one(JobKind::Copy {
                    sources: vec![source],
                    target: target.clone(),
                })
            })
            .collect(),
        JobKind::Move { sources, target } if sources.len() > 1 => sources
            .into_iter()
            .map(|source| {
                one(JobKind::Move {
                    sources: vec![source],
                    target: target.clone(),
                })
            })
            .collect(),
        JobKind::Delete { sources, mode } if sources.len() > 1 => sources
            .into_iter()
            .map(|source| {
                one(JobKind::Delete {
                    sources: vec![source],
                    mode,
                })
            })
            .collect(),
        kind => vec![one(kind)],
    }
}

impl Queue {
    /// Submits the jobs of one operation into the list `list`, in their order: side by side in
    /// their lane, they start one after the other. Every job is checked before any is accepted,
    /// so all are queued or none. Two or more are a batch; one is submitted as
    /// [`Queue::submit_into`] does.
    ///
    /// # Errors
    ///
    /// As [`Queue::submit_into`], for the first job that cannot work.
    pub fn submit_batch(
        &self,
        specs: Vec<JobSpec>,
        list: QueueListId,
    ) -> Result<Vec<JobSnapshot>, OpsError> {
        let specs: Vec<JobSpec> = specs.into_iter().map(|spec| self.marked(spec)).collect();
        if specs.len() < 2 {
            return specs
                .into_iter()
                .map(|spec| self.submit_into(spec, None, list))
                .collect();
        }
        let backends = specs
            .iter()
            .map(|spec| self.check(&spec.kind))
            .collect::<Result<Vec<_>, _>>()?;
        let at_once = list == AT_ONCE;
        let batch = Batch::new(at_once);
        let store = self.hooks().store;
        let mut tasks = Vec::with_capacity(specs.len());
        let mut snapshots = Vec::with_capacity(specs.len());
        for (spec, backends) in specs.into_iter().zip(backends) {
            let job = self.accept(&spec, list, Some(&batch))?;
            batch.join(&job);
            snapshots.push(job.snapshot());
            let lane = match &batch.turns {
                // Every job of the batch in the one place, whatever its backend.
                Some(turns) => Some(turns.enter(&BackendId::Local, Arc::clone(&job))),
                None => self.enter_lane(&job, &spec.kind),
            };
            let resume = Resumption {
                store: store.clone(),
                ..Resumption::default()
            };
            let setup = self.setup(&job, spec.options, backends, None, resume);
            tasks.push((job, spec.kind, lane, setup));
        }
        // The store keeps them one after the other: a restart finds them in their order.
        tokio::spawn(async move {
            for (job, kind, lane, mut setup) in tasks {
                task::keep(&kind, &mut setup).await;
                tokio::spawn(task::run(job, kind, lane, setup));
            }
        });
        Ok(snapshots)
    }

    /// Puts the jobs of one batch the store kept back into the queue as [`Queue::restore`] does,
    /// siblings again; one job alone is no batch. Each job's result, in their order.
    pub fn restore_batch(&self, saved: Vec<SavedJob>) -> Vec<Result<JobSnapshot, OpsError>> {
        let batch = (saved.len() > 1).then(|| Batch::new(false));
        saved
            .into_iter()
            .map(|saved| self.restore_in(saved, batch.as_ref()))
            .collect()
    }
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;
