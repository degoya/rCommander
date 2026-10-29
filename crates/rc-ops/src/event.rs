//! What a queue announces while its jobs run.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{JobId, JobState, Progress, Question};

/// One change of one job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobEvent {
    pub job: JobId,
    pub change: JobChange,
}

/// What changed. Progress carries the numbers, a question what is asked; for anything else
/// the caller reads the job ([`crate::Queue::job`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum JobChange {
    /// Accepted; waits for its turn.
    Queued,
    /// Its turn has come: counting, then the work.
    Started,
    /// Throttled to about ten a second, and once more when the counting is done.
    Progress {
        progress: Progress,
    },
    /// The job waits for [`crate::Queue::answer`].
    Question {
        question: Question,
    },
    /// The question is answered (or the job was cancelled while it waited).
    Answered,
    Paused,
    Resumed,
    /// Over, in `state` (`completed`, `cancelled` or `failed`).
    Finished {
        state: JobState,
    },
}

/// Where a queue sends its events: the engine forwards them onto its own event stream. Called
/// from the jobs' tasks, never while the queue holds a lock, so the sink may call back into the
/// queue.
pub type EventSink = Arc<dyn Fn(JobEvent) + Send + Sync>;
