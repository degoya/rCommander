//! The queue as a list to steer (RC-010-14): every job with its lane and its place among the
//! waiting ones, the waiting jobs reordered or removed, all jobs paused or resumed at once, the
//! finished ones cleared. Each entry names its list (RC-010-15). The jobs run at once are listed
//! too, as a group of their own and marked, but wait for no place (RC-010-25).

use std::collections::{BTreeMap, HashMap};

use rc_vfs::BackendId;
use serde::{Deserialize, Serialize};

use super::{AT_ONCE, Queue, QueueListId, Reorder, lists::in_a_list, lock};
use crate::{JobChange, JobId, JobSnapshot, JobState, OpsError, Progress};

/// One job of [`Queue::entries`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEntry {
    pub job: JobSnapshot,
    /// The backend the job waits for ([`crate::JobKind::lane`]) - its lane, where one job runs
    /// at a time (a few on a server's connection); `None` for a job that only reads (an archive
    /// test).
    pub lane: Option<BackendId>,
    /// Its place among the jobs waiting in its lane, 0 starting next; `None` for a job that
    /// waits for no lane: it runs, it has finished, it runs at once, or the queue runs every job
    /// at once.
    pub place: Option<usize>,
    /// The list the job belongs to ([`crate::Queue::lists`]); [`AT_ONCE`] for a job run at once.
    pub list: QueueListId,
    /// The job runs at once, in none of the lists (F5, F6, F8 confirmed with OK, RC-010-25): it is
    /// paused, resumed and cancelled, but not moved, put into a list or saved.
    #[serde(default)]
    pub at_once: bool,
}

impl Queue {
    /// Job `id`'s state and progress as [`Queue::job`] reads them, without a copy of the rest of
    /// its snapshot: what a progress row takes on each progress event (RC-080-45).
    ///
    /// # Errors
    ///
    /// `job_not_found`.
    pub fn job_progress(&self, id: JobId) -> Result<(JobState, Progress), OpsError> {
        Ok(self.shared(id)?.progress())
    }

    /// Every job in the order a queue window shows them: the unfinished ones run at once (oldest
    /// first) as a group of their own, then the unfinished ones of the lists that hold their lane
    /// or need none (oldest first), then the waiting ones lane by lane in the order they will
    /// start, then the finished ones (oldest first), those run at once among them. Leaving out the
    /// entries marked [`QueueEntry::at_once`] leaves the lists' jobs in the same order.
    #[must_use]
    pub fn entries(&self) -> Vec<QueueEntry> {
        let waiting = self.inner.lanes.waiting();
        let places: HashMap<JobId, usize> = waiting
            .iter()
            .flat_map(|lane| lane.iter().enumerate().map(|(place, id)| (*id, place)))
            .collect();
        let shared: Vec<_> = lock(&self.inner.jobs).values().cloned().collect();
        let lists: HashMap<JobId, QueueListId> =
            shared.iter().map(|job| (job.id, job.list())).collect();
        let mut jobs: BTreeMap<JobId, JobSnapshot> =
            shared.iter().map(|job| (job.id, job.snapshot())).collect();
        let entry = |job: JobSnapshot, place: Option<usize>| {
            let list = lists.get(&job.id).copied().unwrap_or_default();
            QueueEntry {
                lane: job.kind.lane().map(|path| path.backend.clone()),
                place,
                list,
                at_once: list == AT_ONCE,
                job,
            }
        };
        let open = |at_once: bool| -> Vec<JobId> {
            jobs.values()
                .filter(|job| !job.state.is_finished() && !places.contains_key(&job.id))
                .filter(|job| (lists.get(&job.id) == Some(&AT_ONCE)) == at_once)
                .map(|job| job.id)
                .collect()
        };
        let active: Vec<JobId> = open(true).into_iter().chain(open(false)).collect();
        let mut entries = Vec::with_capacity(jobs.len());
        for id in active {
            if let Some(job) = jobs.remove(&id) {
                entries.push(entry(job, None));
            }
        }
        for id in waiting.into_iter().flatten() {
            if let Some(job) = jobs.remove(&id) {
                let place = places.get(&id).copied();
                entries.push(entry(job, place));
            }
        }
        entries.extend(jobs.into_values().map(|job| entry(job, None)));
        entries
    }

    /// Moves a waiting job within its lane: the order decides which job starts when the lane is
    /// free. Up and down pass over the jobs of other lists, so each list keeps its own order; a
    /// job already first (or last) of its list stays where it is.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_finished`, `job_at_once`, `job_not_waiting` for a job that runs or
    /// waits for no lane.
    pub fn reorder(&self, id: JobId, step: Reorder) -> Result<Vec<QueueEntry>, OpsError> {
        in_a_list(&*self.running(id)?)?;
        self.inner.lanes.reorder(id, step)?;
        Ok(self.entries())
    }

    /// Takes a waiting job out of the queue: it ends as cancelled without having started and
    /// leaves the list. A running job is cancelled with [`Queue::cancel`] instead.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_finished`, `job_at_once` (it is cancelled instead),
    /// `job_not_waiting`.
    pub fn remove(&self, id: JobId) -> Result<Vec<QueueEntry>, OpsError> {
        let job = self.running(id)?;
        in_a_list(&job)?;
        if !self.inner.lanes.is_waiting(id) {
            return Err(OpsError::JobNotWaiting { job: id });
        }
        job.control.cancel();
        lock(&self.inner.jobs).remove(&id);
        Ok(self.entries())
    }

    /// Pauses every unfinished job, the ones run at once among them: the running ones at their
    /// next buffer, the waiting ones before they start.
    #[must_use]
    pub fn pause_all(&self) -> Vec<QueueEntry> {
        for job in self.open_jobs() {
            if job.control.pause() {
                job.emit(JobChange::Paused);
            }
        }
        self.entries()
    }

    /// Resumes every paused job, the ones run at once among them; free lanes start their next
    /// job.
    #[must_use]
    pub fn resume_all(&self) -> Vec<QueueEntry> {
        for job in self.open_jobs() {
            if job.control.resume() {
                job.emit(JobChange::Resumed);
                job.kick_batch();
            }
        }
        self.inner.lanes.kick();
        self.entries()
    }

    /// Forgets every finished job.
    #[must_use]
    pub fn clear_finished(&self) -> Vec<QueueEntry> {
        lock(&self.inner.jobs).retain(|_, job| !job.snapshot().state.is_finished());
        self.entries()
    }

    /// The unfinished jobs of the queue's lists; not the ones run at once.
    pub(super) fn unfinished(&self) -> Vec<std::sync::Arc<super::JobShared>> {
        let mut jobs = self.open_jobs();
        jobs.retain(|job| job.list() != AT_ONCE);
        jobs
    }

    /// Every unfinished job, the ones run at once among them.
    fn open_jobs(&self) -> Vec<std::sync::Arc<super::JobShared>> {
        let jobs: Vec<_> = lock(&self.inner.jobs).values().cloned().collect();
        jobs.into_iter()
            .filter(|job| !job.snapshot().state.is_finished())
            .collect()
    }
}
