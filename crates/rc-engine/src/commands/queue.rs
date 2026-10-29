//! The `queue.*` commands (RC-010-14): the operations queue as the queue window shows and steers
//! it. Each answers with the queue as it is afterwards; a change no job event reports is
//! announced as [`EngineEvent::QueueChanged`]. The jobs selected in the window are moved and taken
//! off together, a list or every list cleared (RC-010-15).

use rc_ops::{JobId, QueueListId, Reorder};

use crate::{Engine, EngineError, EngineEvent, Outcome};

impl Engine {
    /// `queue.list`.
    pub(crate) fn queue_list(&self) -> Outcome {
        Outcome::Queue {
            entries: self.inner.ops.entries(),
        }
    }

    /// `queue.move`.
    pub(crate) fn queue_move(&self, job: JobId, step: Reorder) -> Result<Outcome, EngineError> {
        let entries = self.inner.ops.reorder(job, step)?;
        self.publish(EngineEvent::QueueChanged);
        Ok(Outcome::Queue { entries })
    }

    /// `queue.remove`: the job's own end comes as its `finished` event.
    pub(crate) fn queue_remove(&self, job: JobId) -> Result<Outcome, EngineError> {
        let entries = self.inner.ops.remove(job)?;
        self.publish(EngineEvent::QueueChanged);
        Ok(Outcome::Queue { entries })
    }

    /// `queue.move_many`.
    pub(crate) fn queue_move_many(
        &self,
        jobs: &[JobId],
        step: Reorder,
    ) -> Result<Outcome, EngineError> {
        let entries = self.inner.ops.reorder_many(jobs, step)?;
        self.publish(EngineEvent::QueueChanged);
        Ok(Outcome::Queue { entries })
    }

    /// `queue.discard`: a started job's own end comes as its `finished` event.
    pub(crate) fn queue_discard(&self, jobs: &[JobId]) -> Result<Outcome, EngineError> {
        let entries = self.inner.ops.discard(jobs)?;
        self.publish(EngineEvent::QueueChanged);
        Ok(Outcome::Queue { entries })
    }

    /// `queue.clear`: the list `list`, or every list.
    pub(crate) fn queue_clear(
        &self,
        list: QueueListId,
        all_lists: bool,
    ) -> Result<Outcome, EngineError> {
        let entries = self.inner.ops.clear((!all_lists).then_some(list))?;
        self.publish(EngineEvent::QueueChanged);
        Ok(Outcome::Queue { entries })
    }

    /// `queue.pause_all`: every job announces its own pause.
    pub(crate) fn queue_pause_all(&self) -> Outcome {
        Outcome::Queue {
            entries: self.inner.ops.pause_all(),
        }
    }

    /// `queue.resume_all`.
    pub(crate) fn queue_resume_all(&self) -> Outcome {
        Outcome::Queue {
            entries: self.inner.ops.resume_all(),
        }
    }

    /// `queue.clear_finished`.
    pub(crate) fn queue_clear_finished(&self) -> Outcome {
        let entries = self.inner.ops.clear_finished();
        self.publish(EngineEvent::QueueChanged);
        Outcome::Queue { entries }
    }

    /// `queue.window`: the window opens the queue window.
    pub(crate) fn queue_window(&self) -> Outcome {
        self.publish(EngineEvent::QueueWindow);
        self.queue_list()
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
