//! Several named lists in one queue (RC-010-15): "Backup NAS", "Upload web server", each filled,
//! ordered, started and stopped on its own. Every job belongs to one list - the default list
//! unless it was put into another - and moves between them.
//!
//! The lanes and limits stay the queue's (`lanes.rs`): two lists that write to one disk or one
//! server wait in its one lane, so they never overload it together, while lists on different
//! targets run side by side. A stopped list holds its jobs that have not started - they stay
//! queued and let the other lists' jobs pass - and stopping it pauses the ones that run; starting
//! it lets them all go. A new list starts stopped, so it is filled first; the default list is
//! started and is neither renamed nor deleted.

use serde::{Deserialize, Serialize};

use super::{JobShared, Queue, lock};
use crate::{JobChange, JobId, JobSpec, JobState, OpsError, QueueEntry};

/// A list's number, unique for the life of one queue.
pub type QueueListId = u64;

/// The list every job goes into unless another is named.
pub const DEFAULT_LIST: QueueListId = 0;

/// No list: a job submitted "into" it runs at once, in no lane and in none of the queue's
/// lists - Total Commander's operation confirmed with OK, which never waits in the background
/// queue; only F2 puts one there (owner, 2026-10-18). Its jobs are [`Queue::entries`] of their
/// own, marked `at_once`, and paused, resumed and cancelled with the whole queue (RC-010-25); no
/// job is moved into it, none of its jobs is moved, put into a list or saved, and no command
/// takes it for a list. The jobs of one operation still run one after the other (`batch.rs`).
pub const AT_ONCE: QueueListId = QueueListId::MAX;

/// The longest list name, in characters.
pub const MAX_LIST_NAME: usize = 100;

/// One list of [`Queue::lists`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueList {
    pub id: QueueListId,
    /// Empty for the default list, which the window names itself.
    pub name: String,
    /// Its jobs start as their lanes free up; a stopped list holds them.
    pub started: bool,
    /// Its jobs that have not finished.
    pub jobs: usize,
}

/// The lists, in the order they were made, the default list first.
pub(super) struct ListBoard {
    lists: Vec<(QueueListId, String, bool)>,
    next: QueueListId,
}

impl Default for ListBoard {
    fn default() -> Self {
        Self {
            lists: vec![(DEFAULT_LIST, String::new(), true)],
            next: DEFAULT_LIST + 1,
        }
    }
}

impl ListBoard {
    /// Whether the list `list` is started.
    pub(super) fn started(&self, list: QueueListId) -> Result<bool, OpsError> {
        if list == AT_ONCE {
            return Ok(true);
        }
        self.lists
            .iter()
            .find(|(id, ..)| *id == list)
            .map(|(_, _, started)| *started)
            .ok_or(OpsError::QueueListNotFound { list })
    }

    /// Whether the list `list` is started, for a command that takes a list of its own: the jobs
    /// run at once are in none.
    pub(super) fn target(&self, list: QueueListId) -> Result<bool, OpsError> {
        if list == AT_ONCE {
            return Err(OpsError::QueueListNotFound { list });
        }
        self.started(list)
    }

    fn get_mut(&mut self, list: QueueListId) -> Result<&mut (QueueListId, String, bool), OpsError> {
        self.lists
            .iter_mut()
            .find(|(id, ..)| *id == list)
            .ok_or(OpsError::QueueListNotFound { list })
    }
}

/// A list's name, trimmed: not blank, at most [`MAX_LIST_NAME`] characters, no control
/// character. Saved lists (`rc-db`) take the same names.
///
/// # Errors
///
/// `queue_name_invalid`.
pub fn check_list_name(name: &str) -> Result<String, OpsError> {
    let name = name.trim();
    let reason = if name.is_empty() {
        "empty"
    } else if name.chars().count() > MAX_LIST_NAME {
        "too long"
    } else if name.chars().any(char::is_control) {
        "control character"
    } else {
        return Ok(name.to_owned());
    };
    Err(OpsError::QueueNameInvalid { reason })
}

/// `job_at_once` for a job run at once: it waits for no place in a list (RC-010-25).
pub(super) fn in_a_list(job: &JobShared) -> Result<(), OpsError> {
    if job.list() == AT_ONCE {
        return Err(OpsError::JobAtOnce { job: job.id });
    }
    Ok(())
}

/// What [`Queue::list_specs`] answers: each saveable job with its spec, and the jobs left out.
pub type ListSpecs = (Vec<(JobId, JobSpec)>, Vec<JobId>);

impl Queue {
    /// Every list in the order they were made, the default list first.
    #[must_use]
    pub fn lists(&self) -> Vec<QueueList> {
        let board: Vec<(QueueListId, String, bool)> = lock(&self.inner.lists).lists.clone();
        let jobs = self.unfinished();
        board
            .into_iter()
            .map(|(id, name, started)| QueueList {
                id,
                name,
                started,
                jobs: jobs.iter().filter(|job| job.list() == id).count(),
            })
            .collect()
    }

    /// One list.
    ///
    /// # Errors
    ///
    /// `queue_list_not_found`.
    pub fn list(&self, list: QueueListId) -> Result<QueueList, OpsError> {
        self.lists()
            .into_iter()
            .find(|found| found.id == list)
            .ok_or(OpsError::QueueListNotFound { list })
    }

    /// A new, empty list named `name`, stopped until [`Queue::start_list`].
    ///
    /// # Errors
    ///
    /// `queue_name_invalid`.
    pub fn create_list(&self, name: &str) -> Result<QueueList, OpsError> {
        let name = check_list_name(name)?;
        let id = {
            let mut board = lock(&self.inner.lists);
            let id = board.next;
            board.next += 1;
            board.lists.push((id, name, false));
            id
        };
        self.list(id)
    }

    /// Names the list `list` anew.
    ///
    /// # Errors
    ///
    /// `queue_list_not_found`, `queue_list_fixed` for the default list, `queue_name_invalid`.
    pub fn rename_list(&self, list: QueueListId, name: &str) -> Result<QueueList, OpsError> {
        let name = check_list_name(name)?;
        if list == DEFAULT_LIST {
            return Err(OpsError::QueueListFixed);
        }
        lock(&self.inner.lists).get_mut(list)?.1 = name;
        self.list(list)
    }

    /// Removes the list `list`; its jobs that have not finished go to the default list, nothing
    /// is cancelled.
    ///
    /// # Errors
    ///
    /// `queue_list_not_found`, `queue_list_fixed` for the default list.
    pub fn delete_list(&self, list: QueueListId) -> Result<Vec<QueueList>, OpsError> {
        if list == DEFAULT_LIST {
            return Err(OpsError::QueueListFixed);
        }
        let moved = {
            let mut board = lock(&self.inner.lists);
            board.started(list)?;
            board.lists.retain(|(id, ..)| *id != list);
            let started = board.started(DEFAULT_LIST)?;
            let jobs = self.members(list);
            for job in &jobs {
                job.set_list(DEFAULT_LIST);
                job.control.hold(!started);
            }
            jobs
        };
        if !moved.is_empty() {
            self.inner.lanes.kick();
        }
        Ok(self.lists())
    }

    /// Starts the list `list`: its held jobs wait for their lanes again, its paused ones resume.
    ///
    /// # Errors
    ///
    /// `queue_list_not_found`.
    pub fn start_list(&self, list: QueueListId) -> Result<QueueList, OpsError> {
        for job in self.mark_list(list, true)? {
            job.control.hold(false);
            if job.control.resume() {
                job.emit(JobChange::Resumed);
            }
        }
        self.inner.lanes.kick();
        self.list(list)
    }

    /// Stops the list `list`: its jobs that have not started are held, the ones that run pause
    /// at their next buffer.
    ///
    /// # Errors
    ///
    /// `queue_list_not_found`.
    pub fn pause_list(&self, list: QueueListId) -> Result<QueueList, OpsError> {
        for job in self.mark_list(list, false)? {
            job.control.hold(true);
            if job.snapshot().state != JobState::Queued && job.control.pause() {
                job.emit(JobChange::Paused);
            }
        }
        self.list(list)
    }

    /// Puts the job `job` into the list `list`: held while that list is stopped, when it has not
    /// started yet; a job that runs runs on. Within its lane it keeps its place.
    ///
    /// # Errors
    ///
    /// `job_not_found`, `job_finished`, `job_at_once`, `queue_list_not_found`.
    pub fn assign(&self, job: JobId, list: QueueListId) -> Result<Vec<QueueEntry>, OpsError> {
        let shared = self.running(job)?;
        in_a_list(&shared)?;
        {
            let board = lock(&self.inner.lists);
            let started = board.target(list)?;
            shared.set_list(list);
            shared.control.hold(!started);
        }
        self.inner.lanes.kick();
        Ok(self.entries())
    }

    /// The jobs of the list `list` that have not finished, each with its id as it would be
    /// submitted again, in the order the queue window shows them; the ones that cannot be saved
    /// ([`is_saveable`]) are left out and named by id.
    ///
    /// # Errors
    ///
    /// `queue_list_not_found`, also for [`AT_ONCE`]: the jobs run at once are never saved.
    ///
    /// [`is_saveable`]: crate::is_saveable
    pub fn list_specs(&self, list: QueueListId) -> Result<ListSpecs, OpsError> {
        lock(&self.inner.lists).target(list)?;
        let mut specs = Vec::new();
        let mut left_out = Vec::new();
        for entry in self.entries() {
            if entry.list != list || entry.job.state.is_finished() {
                continue;
            }
            if crate::is_saveable(&entry.job.kind) {
                let spec = JobSpec {
                    kind: entry.job.kind,
                    options: entry.job.options,
                };
                specs.push((entry.job.id, spec));
            } else {
                left_out.push(entry.job.id);
            }
        }
        Ok((specs, left_out))
    }

    /// Sets whether the list `list` is started and, under the same lock, takes its unfinished
    /// jobs: a job accepted meanwhile is either among them or saw the new state.
    fn mark_list(
        &self,
        list: QueueListId,
        started: bool,
    ) -> Result<Vec<std::sync::Arc<JobShared>>, OpsError> {
        let mut board = lock(&self.inner.lists);
        board.get_mut(list)?.2 = started;
        Ok(self.members(list))
    }

    /// The unfinished jobs of `list`.
    fn members(&self, list: QueueListId) -> Vec<std::sync::Arc<JobShared>> {
        self.unfinished()
            .into_iter()
            .filter(|job| job.list() == list)
            .collect()
    }
}

#[cfg(test)]
#[path = "lists_tests.rs"]
mod tests;
