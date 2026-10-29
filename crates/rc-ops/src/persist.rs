//! Transfer jobs kept across a restart (RC-060-07).
//!
//! A copy or move to or from a server, or from disk to disk (RC-080-04), is kept by a
//! [`TransferStore`] - the engine's is the table `transfer_jobs` of `rc-db` - from the moment it
//! is submitted until it is over, a job still waiting for its turn too: the job as submitted
//! (sources, target, options; paths and connection ids, never a secret) and, for each file of at
//! least [`MARK_MIN_BYTES`], a [`FileMark`] of the file being written. A job that completes, is
//! cancelled or fails is forgotten; one the app left in the middle - closed, crashed, a machine
//! that went to sleep - stays, and the next start puts it back into the queue as a paused job
//! ([`crate::Queue::restore`]) that the person resumes or cancels.
//!
//! The mark is what makes a resume after a restart safe: the part file is continued only while
//! the source still has the size and the modification time it had when the file started; a
//! changed source starts the file over. A store that fails costs only that resume - the job
//! itself runs on.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use rc_vfs::Entry;
use serde::{Deserialize, Serialize};

use crate::{JobKind, JobSpec, staging::is_resumable};

/// The smallest file a running job marks: a smaller one starts over after a restart, which costs
/// less than one database write per file.
pub const MARK_MIN_BYTES: u64 = 1024 * 1024;

/// The file a kept job was writing: enough to tell, after a restart, whether its part file still
/// belongs to an unchanged source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMark {
    /// The file, on the job's source backend.
    pub source: PathBuf,
    /// Where it lands, on the target backend; its part file is `<target>.rcpart`.
    pub target: PathBuf,
    /// The source's size when the file started.
    pub size: u64,
    /// The source's modification time then, in nanoseconds since 1970; `None` when unknown.
    #[serde(default)]
    pub modified_ns: Option<u64>,
    /// The job replaces an existing target with it (an "overwrite" answer).
    #[serde(default)]
    pub replace: bool,
}

impl FileMark {
    pub(crate) fn new(source: &Path, entry: &Entry, target: &Path, replace: bool) -> Self {
        Self {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            size: entry.size,
            modified_ns: unix_nanos(entry.modified),
            replace,
        }
    }

    /// Whether the mark is the file `source` going to `target`.
    pub(crate) fn is_of(&self, source: &Path, target: &Path) -> bool {
        self.source == source && self.target == target
    }

    /// Whether `entry`, the source now, is what the mark saw.
    pub(crate) fn unchanged(&self, entry: &Entry) -> bool {
        self.size == entry.size && self.modified_ns == unix_nanos(entry.modified)
    }
}

/// `time` in nanoseconds since 1970; `None` when unknown or out of range.
pub(crate) fn unix_nanos(time: Option<SystemTime>) -> Option<u64> {
    let since = time?.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(since.as_nanos()).ok()
}

/// Where the queue keeps its transfer jobs ([`is_kept`]). Each call is
/// awaited by the job, so it should be quick; a failure is the store's to report (the engine
/// logs it) and only costs the resume after a restart.
#[async_trait]
pub trait TransferStore: Send + Sync {
    /// A kept job starts: the id of its record, `None` when it could not be written.
    async fn save(&self, spec: &JobSpec) -> Option<i64>;
    /// [`TransferStore::save`] for a job of the batch `batch` (RC-010-14), which a store that
    /// keeps it hands back so [`crate::Queue::restore_batch`] makes the jobs siblings again; the
    /// jobs of a batch are saved one after the other, in their order.
    async fn save_in(&self, spec: &JobSpec, batch: Option<crate::BatchId>) -> Option<i64> {
        let _ = batch;
        self.save(spec).await
    }
    /// The job of `record` now writes the file `mark`.
    async fn mark(&self, record: i64, mark: &FileMark);
    /// The job of `record` is over - completed, cancelled or failed: its record goes.
    async fn forget(&self, record: i64);
}

/// A job a [`TransferStore`] kept, to put back into the queue with [`crate::Queue::restore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedJob {
    /// The id of its record.
    pub record: i64,
    pub spec: JobSpec,
    /// The file it was writing, when it was marked.
    pub mark: Option<FileMark>,
}

/// Whether a job of `kind` is kept: a copy or move to or from a server, or from disk to disk
/// (RC-080-04) - the transfers that write through a part file and can resume.
#[must_use]
pub fn is_kept(kind: &JobKind) -> bool {
    match kind {
        JobKind::Copy { sources, target } | JobKind::Move { sources, target } => sources
            .iter()
            .any(|source| is_resumable(&source.backend, &target.backend)),
        JobKind::CopyAs { source, target } | JobKind::MoveAs { source, target } => {
            is_resumable(&source.backend, &target.backend)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "persist_tests.rs"]
mod tests;
