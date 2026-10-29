//! Several jobs at once (RC-010-15, owner 2026-10-18): the jobs selected in the queue window moved
//! up, down, to the front or to the back of their lanes keeping their order among themselves, put
//! into another list, or taken off the queue - a waiting one removed, a started one cancelled as a
//! single cancel does it, a finished one forgotten - and a whole list, or every list, cleared.
//!
//! A selection may hold part of a batch (`batch.rs`): the jobs taken leave the queue or change
//! their list one by one, as a single cancel or move does, and the siblings left keep their batch.

use std::sync::Arc;

use super::{AT_ONCE, JobShared, Queue, QueueListId, Reorder, lists::in_a_list, lock};
use crate::{JobId, OpsError, QueueEntry};

impl Queue {
    /// Moves the waiting jobs among `jobs` within their lanes, keeping their order among
    /// themselves; the others among them (running, finished, run at once) stay where they are.
    /// Up and down pass over the jobs of other lists, as [`Queue::reorder`] does.
    ///
    /// # Errors
    ///
    /// `job_not_found` for an id the queue does not know; nothing moves then.
    pub fn reorder_many(&self, jobs: &[JobId], step: Reorder) -> Result<Vec<QueueEntry>, OpsError> {
        self.known(jobs)?;
        self.inner.lanes.reorder_many(jobs, step);
        Ok(self.entries())
    }

    /// Puts `jobs` into the list `list` as [`Queue::assign`] puts one: held while that list is
    /// stopped when it has not started, run on when it runs; a finished one only changes its
    /// list, so that it is shown there.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_at_once` for a job run at once, `queue_list_not_found`; nothing
    /// moves then.
    pub fn assign_many(
        &self,
        jobs: &[JobId],
        list: QueueListId,
    ) -> Result<Vec<QueueEntry>, OpsError> {
        let shared = self.movable(jobs)?;
        {
            let board = lock(&self.inner.lists);
            let started = board.target(list)?;
            for job in &shared {
                job.set_list(list);
                if !job.snapshot().state.is_finished() {
                    job.control.hold(!started);
                }
            }
        }
        self.inner.lanes.kick();
        Ok(self.entries())
    }

    /// Takes `jobs` off the queue: a waiting one ends as cancelled without having started (as
    /// [`Queue::remove`]), a started one - running, paused or asking - is cancelled as
    /// [`Queue::cancel`] cancels it (the file being written is removed; a transfer's record is
    /// dealt with as a single cancel deals with it), a finished one is forgotten. None of them is
    /// listed afterwards; a started one's end still comes as its `finished` event.
    ///
    /// # Errors
    ///
    /// `job_not_found`; nothing is taken off then.
    pub fn discard(&self, jobs: &[JobId]) -> Result<Vec<QueueEntry>, OpsError> {
        let shared = self.known(jobs)?;
        self.take_off(&shared);
        Ok(self.entries())
    }

    /// Takes every job of the list `list` off the queue as [`Queue::discard`] does, finished or
    /// not; with `None` every list's jobs. The jobs run at once are in no list and stay.
    ///
    /// # Errors
    ///
    /// `queue_list_not_found`, also for [`AT_ONCE`].
    pub fn clear(&self, list: Option<QueueListId>) -> Result<Vec<QueueEntry>, OpsError> {
        if let Some(list) = list {
            lock(&self.inner.lists).target(list)?;
        }
        let jobs: Vec<Arc<JobShared>> = lock(&self.inner.jobs)
            .values()
            .filter(|job| job.list() != AT_ONCE && list.is_none_or(|list| job.list() == list))
            .cloned()
            .collect();
        self.take_off(&jobs);
        Ok(self.entries())
    }

    /// Whether every one of `jobs` can go into another list: `job_not_found` for the first the
    /// queue does not know, `job_at_once` for the first run at once.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_at_once`.
    pub fn check_movable(&self, jobs: &[JobId]) -> Result<(), OpsError> {
        self.movable(jobs).map(|_| ())
    }

    /// Every one of `jobs`, none of them run at once.
    fn movable(&self, jobs: &[JobId]) -> Result<Vec<Arc<JobShared>>, OpsError> {
        let shared = self.known(jobs)?;
        shared.iter().try_for_each(|job| in_a_list(job))?;
        Ok(shared)
    }

    /// Every one of `jobs`, or `job_not_found` for the first the queue does not know.
    fn known(&self, jobs: &[JobId]) -> Result<Vec<Arc<JobShared>>, OpsError> {
        let map = lock(&self.inner.jobs);
        jobs.iter()
            .map(|id| {
                map.get(id)
                    .cloned()
                    .ok_or(OpsError::JobNotFound { job: *id })
            })
            .collect()
    }

    /// Cancels the unfinished ones of `jobs` and forgets them all.
    fn take_off(&self, jobs: &[Arc<JobShared>]) {
        for job in jobs {
            if !job.snapshot().state.is_finished() {
                job.control.cancel();
            }
        }
        let mut map = lock(&self.inner.jobs);
        for job in jobs {
            map.remove(&job.id);
        }
    }
}

/// Moves the `chosen` ones of `items` by `step`, keeping their order among themselves: up and
/// down by one place among the items of their own group (`group`) - a chosen item whose place is
/// another chosen one's stays behind it -, to the front or the back as one block. The other items
/// keep their order.
pub(super) fn shift<T>(
    items: &mut Vec<T>,
    mut chosen: Vec<bool>,
    step: Reorder,
    group: impl Fn(&T) -> u64,
) {
    match step {
        Reorder::Up => {
            for at in 0..items.len() {
                if !chosen[at] {
                    continue;
                }
                let own = group(&items[at]);
                let before = items[..at].iter().rposition(|item| group(item) == own);
                if let Some(before) = before.filter(|before| !chosen[*before]) {
                    items.swap(at, before);
                    chosen.swap(at, before);
                }
            }
        }
        Reorder::Down => {
            for at in (0..items.len()).rev() {
                if !chosen[at] {
                    continue;
                }
                let own = group(&items[at]);
                let after = items[at + 1..]
                    .iter()
                    .position(|item| group(item) == own)
                    .map(|after| at + 1 + after);
                if let Some(after) = after.filter(|after| !chosen[*after]) {
                    items.swap(at, after);
                    chosen.swap(at, after);
                }
            }
        }
        Reorder::Front | Reorder::Back => {
            let (mut taken, mut left) = (Vec::new(), Vec::new());
            for (item, chosen) in items.drain(..).zip(chosen) {
                if chosen {
                    taken.push(item);
                } else {
                    left.push(item);
                }
            }
            if step == Reorder::Front {
                taken.append(&mut left);
                *items = taken;
            } else {
                left.append(&mut taken);
                *items = left;
            }
        }
    }
}

#[cfg(test)]
#[path = "bulk_tests.rs"]
mod tests;
