//! What a job is asked to do, and what it reports.

use std::sync::Arc;

use rc_checksum::{Algorithm, Layout};
use rc_rename::RenameStep;
use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use crate::{
    ConflictChoice, ExtractSettings, JobReport, OpsError, PackSettings, Progress, Question,
    staging::is_remote,
};

/// A job's number, unique for the life of one queue.
pub type JobId = u64;

/// The operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum JobKind {
    /// Copies every source into the directory `target` (created when missing). Directories
    /// are copied with their contents and merged into one of the same name.
    Copy {
        sources: Vec<VfsPath>,
        target: VfsPath,
    },
    /// Like `Copy`, then each source file is removed once its copy is complete; within one
    /// backend an entry is renamed instead.
    Move {
        sources: Vec<VfsPath>,
        target: VfsPath,
    },
    /// Removes every source with its contents.
    Delete {
        sources: Vec<VfsPath>,
        mode: DeleteMode,
    },
    /// Creates the directory `path` and any missing parent.
    Mkdir { path: VfsPath },
    /// Renames entries of the directory `dir`, one step after the other in the given order: a
    /// multi-rename's schedule (`rc_rename::schedule`). A step never replaces an entry - one
    /// whose new name exists fails - and a failing step stops the job, since the steps after it
    /// rely on it. Made with [`JobKind::rename`].
    Rename {
        dir: VfsPath,
        /// How many steps there are.
        count: usize,
        /// The steps, shared: out of the serde form, which every snapshot of the job carries.
        #[serde(skip)]
        steps: Arc<Vec<RenameStep>>,
    },
    /// Copies the one entry `source` to `target`, the full path of the copy: Total Commander's
    /// F5 with a new name, Shift+F5 in the same directory. A directory goes with its contents;
    /// the target's directory is created when missing, an existing target is a conflict.
    CopyAs { source: VfsPath, target: VfsPath },
    /// Like `CopyAs`, then the source is removed; within one backend it is renamed (F6 with a
    /// new name, Shift+F6).
    MoveAs { source: VfsPath, target: VfsPath },
    /// Reads every file under `archive` (a location on an archive backend, `/` its root) to
    /// its end, so the format checks its CRCs and stream checksums; writes nothing. A damaged
    /// or encrypted entry is a failure with its code, links and special entries are skipped.
    TestArchive { archive: VfsPath },
    /// Unpacks every archive into the directory `target` (RC-030-03).
    Extract {
        archives: Vec<VfsPath>,
        target: VfsPath,
        #[serde(default)]
        settings: ExtractSettings,
    },
    /// Packs the sources into the new archive `target` - or, with `each_own_archive`, each
    /// source into its own archive in the directory `target` (RC-030-04).
    Pack {
        sources: Vec<VfsPath>,
        target: VfsPath,
        #[serde(default)]
        settings: PackSettings,
    },
    /// Writes checksum files for the entries `names` of the directory `dir` - files, and the
    /// files inside folders - one beside each file or one for all (RC-020-05, `rc-checksum`).
    /// Nothing that exists is overwritten.
    ChecksumCreate {
        dir: VfsPath,
        names: Vec<String>,
        algorithm: Algorithm,
        layout: Layout,
    },
    /// Hashes every file the checksum file `file` lists again and compares; writes nothing.
    /// The result per line is the snapshot's `report`.
    ChecksumVerify { file: VfsPath },
    /// Splits the file `source` into parts of `part_size` bytes with Total Commander's `.crc`
    /// file, in the directory `target`.
    Split {
        source: VfsPath,
        target: VfsPath,
        part_size: u64,
    },
    /// Puts the split file of `first` (a part or its `.crc` file) back together in the
    /// directory `target`, named `name`, else as the `.crc` file says; the CRC is checked.
    Combine {
        first: VfsPath,
        target: VfsPath,
        #[serde(default)]
        name: Option<String>,
    },
}

impl JobKind {
    /// A rename job carrying out `steps` in `dir`.
    #[must_use]
    pub fn rename(dir: VfsPath, steps: Vec<RenameStep>) -> Self {
        Self::Rename {
            dir,
            count: steps.len(),
            steps: Arc::new(steps),
        }
    }

    /// The backend the job waits for, its lane: the one it writes to - the target's, the
    /// sources' for a delete, the directory's for a rename - except a download from a server to
    /// a backend that is none, which waits for the server's connection (RC-060-05). One job runs
    /// at a time per lane, a few per server ([`crate::QueueConfig::per_connection`]).
    #[must_use]
    pub fn lane(&self) -> Option<&VfsPath> {
        match self {
            Self::Copy { sources, target } | Self::Move { sources, target } => {
                Some(transfer_lane(sources.first(), target))
            }
            Self::Delete { sources, .. } => sources.first(),
            Self::Mkdir { path } => Some(path),
            Self::Rename { dir, .. } => Some(dir),
            Self::CopyAs { source, target } | Self::MoveAs { source, target } => {
                Some(transfer_lane(Some(source), target))
            }
            // Reads only: it waits for no writer.
            Self::TestArchive { .. } => None,
            Self::Extract { target, .. } | Self::Pack { target, .. } => Some(target),
            Self::ChecksumCreate { dir, .. } => Some(dir),
            Self::ChecksumVerify { .. } => None,
            Self::Split { target, .. } | Self::Combine { target, .. } => Some(target),
        }
    }
}

/// The lane of a transfer: the source's server when it downloads to a backend that is no
/// server, else the target.
fn transfer_lane<'a>(source: Option<&'a VfsPath>, target: &'a VfsPath) -> &'a VfsPath {
    match source {
        Some(source) if is_remote(&source.backend) && !is_remote(&target.backend) => source,
        _ => target,
    }
}

/// Where deleted entries go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    /// Into the recycle bin; refused with `trash_unavailable` where the backend has none.
    #[default]
    Trash,
    /// Gone for good.
    Permanent,
}

/// What happens when a file fails (no access, disk full, a copy that does not verify).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorPolicy {
    /// The job asks: retry, skip or cancel.
    #[default]
    Ask,
    /// The file is recorded as failed and the job goes on.
    Skip,
    /// The job stops as failed.
    Cancel,
}

/// How a job runs; MCP sets every answer in advance, the GUI asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct JobOptions {
    /// The answer to every conflict; `None` asks for each (until one is answered "for all").
    pub on_conflict: Option<ConflictChoice>,
    /// What a failing file does.
    pub on_error: ErrorPolicy,
    /// Compare BLAKE3 checksums of source and target after each copied file.
    pub verify: bool,
    /// Keep the time stamps and rights of what is copied: the modification time, and the Unix
    /// mode where both sides have one (else the read-only flag). On by default.
    pub keep_attributes: bool,
    /// The job reads no file the queue's read guard keeps from MCP ([`crate::ReadGuards`]):
    /// set by the queue for a job submitted or loaded inside an MCP call, kept across a
    /// restart (RC-080-42); a caller cannot set it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub read_guarded: bool,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self {
            on_conflict: None,
            on_error: ErrorPolicy::default(),
            verify: false,
            keep_attributes: true,
            read_guarded: false,
        }
    }
}

/// A job as submitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSpec {
    pub kind: JobKind,
    #[serde(default)]
    pub options: JobOptions,
}

/// Where a job stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Waiting for an earlier job on the same target backend.
    Queued,
    /// Counting files and bytes.
    Counting,
    Running,
    /// Paused by the caller; holds open files where it stopped.
    Paused,
    /// Waiting for an answer to its question.
    Waiting,
    /// Done; failed files, if any, are in the snapshot's `failures`.
    Completed,
    /// Cancelled by the caller or by a `Cancel` answer to a conflict.
    Cancelled,
    /// Stopped by an error (`on_error: cancel`, or `Cancel` answered to an error).
    Failed,
}

impl JobState {
    /// Whether the job is over.
    #[must_use]
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

/// A file the job could not handle, or the error that stopped it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub path: VfsPath,
    /// The stable error code (`error.<code>` in the catalogue).
    pub code: String,
    pub message: String,
    /// Where the target failed, when it was the target and not the source: the write side of a
    /// copy (RC-060-06), so a copy between two servers says which one went. `path` stays the
    /// file's source. Boxed so the `Err` side of every step stays small.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Box<VfsPath>>,
}

impl Failure {
    pub(crate) fn new(path: &VfsPath, error: &OpsError) -> Self {
        Self {
            path: path.clone(),
            code: error.code().to_owned(),
            message: error.to_string(),
            target: None,
        }
    }
}

/// A job as a caller sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSnapshot {
    pub id: JobId,
    pub kind: JobKind,
    pub options: JobOptions,
    pub state: JobState,
    pub progress: Progress,
    /// The question the job waits on (state `waiting`).
    pub question: Option<Question>,
    /// Files skipped by a conflict answer.
    pub skipped: u64,
    /// Files that failed and were skipped, and for a failed job the error that stopped it.
    pub failures: Vec<Failure>,
    /// What a checksum, split or combine job found and wrote, once it completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<JobReport>,
    /// A paused transfer that continues where it stopped once resumed (RC-060-07): put back
    /// into the queue after a restart ([`crate::Queue::restore`]), or paused after its retries
    /// of a lost connection were spent.
    #[serde(default)]
    pub resumable: bool,
    /// The operation it is one job of, when a copy, move or delete of several sources was
    /// submitted as one job per source (RC-010-14, [`crate::per_item`]): its siblings share it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<crate::BatchId>,
}
