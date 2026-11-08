//! The errors of the queue and of single files, each with a stable code.

use std::{io, path::PathBuf};

use rc_vfs::VfsError;

use crate::JobId;

/// What went wrong: refusing a job or a command at once, or with one file while a job runs
/// (then it reaches the caller as a [`crate::Failure`] or an error question).
///
/// [`OpsError::code`] is the stable form the GUI translates (`error.<code>`) and MCP returns; a
/// file-system failure keeps the code of its [`VfsError`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OpsError {
    /// The file system refused or failed.
    #[error(transparent)]
    Vfs(VfsError),
    /// No job has this id (any more: finished jobs are forgotten after a while).
    #[error("no job {job}")]
    JobNotFound { job: JobId },
    /// The job has finished; it can no longer be paused, resumed, cancelled or answered.
    #[error("job {job} has already finished")]
    JobFinished { job: JobId },
    /// An answer for a question the job is not asking.
    #[error("job {job} is not waiting for this answer")]
    NoQuestion { job: JobId },
    /// The job does not wait for its turn: it runs, or it waits for no lane.
    #[error("job {job} is not waiting in the queue")]
    JobNotWaiting { job: JobId },
    /// No backend is registered for a location of the job.
    #[error("no file system is open for this location")]
    BackendUnavailable,
    /// A job without sources.
    #[error("the job has no sources")]
    NoSources,
    /// The sources of one job lie on different backends.
    #[error("the sources lie on different file systems")]
    MixedSources,
    /// A directory would be copied or moved into itself.
    #[error("{}: the target lies inside the source", path.display())]
    TargetInsideSource { path: PathBuf },
    /// An entry would be copied or moved onto itself.
    #[error("{}: source and target are the same", path.display())]
    SameLocation { path: PathBuf },
    /// The backend has no recycle bin; deleting needs the permanent mode.
    #[error("{}: there is no recycle bin here", path.display())]
    TrashUnavailable { path: PathBuf },
    /// The recycle bin refused the entry.
    #[error("{}: moving to the recycle bin failed: {reason}", path.display())]
    TrashFailed { path: PathBuf, reason: String },
    /// The target has no room left.
    #[error("{}: the disk is full", path.display())]
    DiskFull { path: PathBuf },
    /// The copy reads back different from its source.
    #[error("{}: the copy differs from its source", path.display())]
    VerifyFailed { path: PathBuf },
    /// A file meets a directory of the same name, or the other way round.
    #[error("{}: a file and a directory have the same name", path.display())]
    KindMismatch { path: PathBuf },
    /// A typed file name that is not one plain name every backend can hold.
    #[error("{name:?} is not a valid file name: {reason}")]
    InvalidName { name: String, reason: &'static str },
    /// A dangling link or a special file: nothing a stream can copy. A link to a directory goes
    /// by `ops.copy_links` (RC-080-63).
    #[error("{}: dangling links and special files are not copied", path.display())]
    NotCopyable { path: PathBuf },
    /// A rename job's journal could not record a step; the job stopped before the next one.
    #[error("the rename cannot be recorded for undo: {reason}")]
    JournalFailed { reason: String },
    /// A file to unpack is no archive this program reads.
    #[error("{}: not an archive", path.display())]
    NotAnArchive { path: PathBuf },
    /// An archive entry that would land outside the target, or on a name the target cannot hold
    /// safely: skipped, never written.
    #[error("{}: skipped, unsafe in an archive ({reason})", path.display())]
    UnsafeEntry { path: PathBuf, reason: &'static str },
    /// A link stored in an archive: not created on unpacking.
    #[error("{}: links stored in an archive are not created", path.display())]
    LinkNotExtracted { path: PathBuf },
    /// The archive writer stopped unexpectedly (a panic in an archive crate); nothing is left
    /// under the archive's name.
    #[error("{}: the archive writer stopped unexpectedly", path.display())]
    PackFailed { path: PathBuf },
    /// No list of the queue has this id (RC-010-15).
    #[error("no queue list {list}")]
    QueueListNotFound { list: crate::QueueListId },
    /// The default list is neither renamed nor deleted.
    #[error("the default queue list cannot be renamed or deleted")]
    QueueListFixed,
    /// A list name that is blank, too long or holds a control character.
    #[error("invalid queue list name: {reason}")]
    QueueNameInvalid { reason: &'static str },
    /// The job runs at once, outside the queue's lists (`AT_ONCE`, RC-010-25): it waits for no
    /// place, so it is neither moved nor put into a list nor saved; it is paused or cancelled.
    #[error("job {job} runs at once, outside the queue's lists")]
    JobAtOnce { job: JobId },
    /// A case-only rename failed and so did putting the old name back: the entry stays under
    /// its temporary name `path`, and a retry finishes the rename from there (RC-080-37).
    #[error("{}: left under this temporary name; a retry finishes the rename", path.display())]
    RenameUndoFailed { path: PathBuf },
    /// The job's read guard keeps the file's contents from MCP (RC-080-42): nothing of it is
    /// read.
    #[error("{}: its contents never leave through MCP ({reason})", path.display())]
    Sensitive { path: PathBuf, reason: &'static str },
    /// A link to a folder copied as a link (`ops.copy_links = link`, RC-080-63) onto a target
    /// that makes no links (FTP, an archive, a drive without them).
    #[error("{}: the target cannot hold links; the link is not copied", path.display())]
    LinksUnsupported { path: PathBuf },
    /// A link to a folder that lies on its own way (a folder above it, or one the copy came
    /// through): following it would copy without end, so it is not followed (RC-080-63).
    #[error("{}: the link leads back into its own path; not followed", path.display())]
    LinkLoop { path: PathBuf },
    /// An archive that is not changed in place of its kind (RC-110-05): a compressed tar, a
    /// single compressed file, a zip behind a prefix (a self-extractor), a tar with sparse files.
    #[error("{}: this archive cannot be changed; unpack it and pack it anew", path.display())]
    ArchiveNotModifiable { path: PathBuf },
    /// An archive with encrypted entries or an encrypted header, or an encrypted entry to add
    /// from another archive: never changed, no password is asked for to write (RC-110-05).
    #[error("{}: the archive holds encrypted entries; it is not changed", path.display())]
    ArchiveEncrypted { path: PathBuf },
    /// The archive file changed while the update ran (another job, another program): the new
    /// archive is thrown away, the changed one stays (RC-110-05).
    #[error("{}: the archive changed meanwhile; nothing was changed", path.display())]
    ArchiveChanged { path: PathBuf },
    /// The archive file is open elsewhere and cannot be replaced: it stays as it was.
    #[error("{}: the archive is in use and cannot be replaced", path.display())]
    ArchiveInUse { path: PathBuf },
    /// An archive inside an archive or on a server: changed only on a local drive, as packing.
    #[error("{}: only an archive on a local drive can be changed", path.display())]
    ArchiveNotLocal { path: PathBuf },
    /// The archive file is read-only.
    #[error("{}: the archive file is read-only", path.display())]
    ArchiveFileReadOnly { path: PathBuf },
    /// The drive has not room for the new archive beside the old one.
    #[error("{}: not enough space: {needed} bytes needed, {free} free", path.display())]
    NotEnoughSpace {
        path: PathBuf,
        needed: u64,
        free: u64,
    },
}

impl OpsError {
    /// Every code of the queue's own variants; a file-system failure carries one of
    /// [`VfsError::ALL_CODES`] instead.
    pub const ALL_CODES: &[&str] = &[
        "job_not_found",
        "job_finished",
        "no_question",
        "job_not_waiting",
        "backend_unavailable",
        "no_sources",
        "mixed_sources",
        "target_inside_source",
        "same_location",
        "trash_unavailable",
        "trash_failed",
        "disk_full",
        "verify_failed",
        "kind_mismatch",
        "invalid_name",
        "not_copyable",
        "journal_failed",
        "not_an_archive",
        "unsafe_entry",
        "link_not_extracted",
        "pack_failed",
        "queue_list_not_found",
        "queue_list_fixed",
        "queue_name_invalid",
        "job_at_once",
        "rename_undo_failed",
        "sensitive_path",
        "links_unsupported",
        "link_loop",
        "archive_not_modifiable",
        "archive_encrypted",
        "archive_changed",
        "archive_in_use",
        "archive_not_local",
        "archive_file_read_only",
        "not_enough_space",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Vfs(error) => error.code(),
            Self::JobNotFound { .. } => "job_not_found",
            Self::JobFinished { .. } => "job_finished",
            Self::NoQuestion { .. } => "no_question",
            Self::JobNotWaiting { .. } => "job_not_waiting",
            Self::BackendUnavailable => "backend_unavailable",
            Self::NoSources => "no_sources",
            Self::MixedSources => "mixed_sources",
            Self::TargetInsideSource { .. } => "target_inside_source",
            Self::SameLocation { .. } => "same_location",
            Self::TrashUnavailable { .. } => "trash_unavailable",
            Self::TrashFailed { .. } => "trash_failed",
            Self::DiskFull { .. } => "disk_full",
            Self::VerifyFailed { .. } => "verify_failed",
            Self::KindMismatch { .. } => "kind_mismatch",
            Self::InvalidName { .. } => "invalid_name",
            Self::NotCopyable { .. } => "not_copyable",
            Self::JournalFailed { .. } => "journal_failed",
            Self::NotAnArchive { .. } => "not_an_archive",
            Self::UnsafeEntry { .. } => "unsafe_entry",
            Self::LinkNotExtracted { .. } => "link_not_extracted",
            Self::PackFailed { .. } => "pack_failed",
            Self::QueueListNotFound { .. } => "queue_list_not_found",
            Self::QueueListFixed => "queue_list_fixed",
            Self::QueueNameInvalid { .. } => "queue_name_invalid",
            Self::JobAtOnce { .. } => "job_at_once",
            Self::RenameUndoFailed { .. } => "rename_undo_failed",
            Self::Sensitive { .. } => "sensitive_path",
            Self::LinksUnsupported { .. } => "links_unsupported",
            Self::LinkLoop { .. } => "link_loop",
            Self::ArchiveNotModifiable { .. } => "archive_not_modifiable",
            Self::ArchiveEncrypted { .. } => "archive_encrypted",
            Self::ArchiveChanged { .. } => "archive_changed",
            Self::ArchiveInUse { .. } => "archive_in_use",
            Self::ArchiveNotLocal { .. } => "archive_not_local",
            Self::ArchiveFileReadOnly { .. } => "archive_file_read_only",
            Self::NotEnoughSpace { .. } => "not_enough_space",
        }
    }

    /// An I/O error of a stream on `path`, typed like the backend's own errors; a full disk
    /// becomes [`OpsError::DiskFull`].
    pub(crate) fn from_io(error: io::Error, path: impl Into<PathBuf>) -> Self {
        Self::from(VfsError::from_io(error, path))
    }
}

impl From<VfsError> for OpsError {
    /// Keeps the backend's error, except a full disk, which every backend reports as plain I/O.
    fn from(error: VfsError) -> Self {
        match error {
            VfsError::Io { path, source }
                if matches!(
                    source.kind(),
                    io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded
                ) =>
            {
                Self::DiskFull { path }
            }
            other => Self::Vfs(other),
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
