//! rCommander's file operations: a queue of copy, move, delete, mkdir and multi-rename jobs - and
//! the test of an archive, which only reads, unpacking (`extract.rs`, RC-030-03) and packing
//! (`pack/`, RC-030-04) archives - that work only against the [`rc_vfs::Vfs`] trait (a pack writes
//! its archive as a local file). A copy or move goes into a directory, or for one entry to a path
//! of its own (`CopyAs`, `MoveAs`: a new name, a rename).
//!
//! The core never looks at a local path: a copy is `open_read` on the source backend streamed
//! through a few buffers into `open_write` on the target backend, reading and writing at once
//! (`stream_pair.rs`, RC-060-06), so local to local, local to a server and server to server
//! (with no file in between, two servers or one) are the same code. A [`Queue`] holds
//! the backends by [`rc_vfs::BackendId`] ([`Queue::register`]), runs jobs one after another per
//! target backend - a few at once per server, a file written there under a part name until it is
//! complete (RC-060-05) - or in parallel ([`Scheduling`]), and reports through an [`EventSink`]:
//! progress (throttled to about ten events a second), questions and the end of each job.
//!
//! A job that meets an existing target or a failing file asks its caller a [`Question`] and
//! waits for [`Queue::answer`]; [`JobOptions`] answers in advance (MCP always does). Pause,
//! resume and cancel act between two buffers of a file; a cancelled or failed file is removed,
//! never left half written. Headless: the GUI's dialogs (RC-010-07) answer through the engine.
//!
//! A copy to or from a server resumes (RC-060-07, `resume.rs`): a lost connection keeps its part
//! file, is retried with a growing wait on a backend the [`Reconnect`] hook opens again, and the
//! file continues from where the part ends once that is safe; a [`TransferStore`] keeps the job
//! across a restart, and [`Queue::restore`] puts it back, paused.
//!
//! The queue holds several named lists (RC-010-15, `queue/lists.rs`), each started and stopped on
//! its own while the lanes and limits stay shared; a list's copies and moves are taken out as
//! specs to save ([`Queue::list_specs`]) and put back with [`Queue::load`]. An operation of
//! several sources can run as one job per source ([`per_item`], [`Queue::submit_batch`]), whose
//! answers "for all" and cancels reach each other as they did inside one job (RC-010-14).
//!
//! A rename job ([`JobKind::rename`]) carries out a multi-rename's steps in their order, never
//! onto an existing name, and stops at the first failing step; a [`StepJournal`] given to
//! [`Queue::submit_journaled`] records each step before the next one starts (the undo log).
//!
//! Checksum files, their check, split and combine (`checksum.rs`, RC-020-05) are jobs too: the
//! work is `rc-checksum`'s, the queue gives it pause, cancel and progress, and what it found is
//! the snapshot's [`JobReport`].

mod archive_test;
mod attributes;
mod case_rename;
mod checksum;
mod conflict;
mod control;
mod copy;
mod delete;
mod dir_size;
mod error;
mod event;
mod extract;
mod job;
mod journal;
pub mod mask;
mod name;
mod pack;
mod persist;
mod progress;
mod queue;
mod read_guard;
mod rename;
mod resume;
mod runner;
mod staging;
mod stream_pair;
pub mod temp_name;
mod trash;
mod verify;
mod walk;

pub use attributes::{
    ApplyTo, AttributeFailure, AttributeReport, MAX_LISTED_FAILURES, set_properties_tree,
};
pub use checksum::{JobReport, MAX_REPORT_LINES};
pub use conflict::{Answer, ConflictChoice, EntryInfo, ErrorChoice, Question};
pub use dir_size::{DirUsage, dir_size, dir_sizes};
pub use error::OpsError;
pub use event::{EventSink, JobChange, JobEvent};
pub use extract::ExtractSettings;
pub use job::{
    DeleteMode, ErrorPolicy, Failure, JobId, JobKind, JobOptions, JobSnapshot, JobSpec, JobState,
};
pub use journal::StepJournal;
pub use name::{check_name, check_target, keeps_windows_rules};
pub use pack::{PackFormat, PackSettings};
pub use persist::{FileMark, MARK_MIN_BYTES, SavedJob, TransferStore, is_kept};
pub use progress::{Meter, Progress};
pub use queue::{
    AT_ONCE, Backend, BatchId, DEFAULT_BUFFER_SIZE, DEFAULT_LIST, DEFAULT_MAX_RUNNING,
    DEFAULT_PER_CONNECTION, MAX_LIST_NAME, Queue, QueueConfig, QueueEntry, QueueList, QueueListId,
    Reorder, Scheduling, check_list_name, is_saveable, per_item,
};
pub use read_guard::{ReadGuard, ReadGuards};
pub use resume::{
    DEFAULT_RETRY_ATTEMPTS, DEFAULT_RETRY_DELAY, Reconnect, RetryPolicy, TAIL_CHECK,
    is_connection_lost,
};
pub use trash::{SystemTrash, Trash};
