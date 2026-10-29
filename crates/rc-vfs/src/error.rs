//! The errors of every backend, typed by what went wrong rather than by where.
//!
//! Each variant has a stable [`VfsError::code`]: the UI picks its message by it, and the MCP
//! server returns it to agents, so a code never changes once released.

use std::{io, path::PathBuf};

/// The result of every [`crate::Vfs`] operation.
pub type Result<T, E = VfsError> = std::result::Result<T, E>;

/// What went wrong, with the path it went wrong on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VfsError {
    /// The path, or a directory on the way to it, does not exist.
    #[error("{}: not found", path.display())]
    NotFound { path: PathBuf },
    /// The backend refused the operation.
    #[error("{}: permission denied", path.display())]
    PermissionDenied { path: PathBuf },
    /// Something already exists where the operation would create.
    #[error("{}: already exists", path.display())]
    AlreadyExists { path: PathBuf },
    /// A directory was expected; the path is something else.
    #[error("{}: not a directory", path.display())]
    NotADirectory { path: PathBuf },
    /// A file was expected; the path is a directory.
    #[error("{}: is a directory", path.display())]
    IsADirectory { path: PathBuf },
    /// A directory removed without `recursive` still has entries.
    #[error("{}: directory not empty", path.display())]
    DirectoryNotEmpty { path: PathBuf },
    /// The path cannot name anything on this backend.
    #[error("{}: invalid path ({reason})", path.display())]
    InvalidPath { path: PathBuf, reason: &'static str },
    /// The backend or platform cannot do this operation at all.
    #[error("{}: {operation} is not supported here", path.display())]
    Unsupported {
        path: PathBuf,
        operation: &'static str,
    },
    /// Any other I/O failure.
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The entry, or the whole archive, is encrypted and no password was given.
    #[error("{}: a password is required", path.display())]
    PasswordRequired { path: PathBuf },
    /// An archive passes one of its safety limits (entry count, unpacked size, compression
    /// ratio): an archive bomb, or a setting set too low.
    #[error("{}: archive limit exceeded ({limit})", path.display())]
    ArchiveLimit { path: PathBuf, limit: &'static str },
    /// Unpacking needs more memory than [`crate::ArchiveLimits::max_decoder_memory`] allows: the
    /// dictionary an xz or 7z header asks for, refused before it is set aside (RC-080-35).
    #[error("{}: unpacking needs more memory than the decoder memory limit", path.display())]
    ArchiveDecoderMemory { path: PathBuf },
    /// The archive is damaged, or not what its first bytes claim.
    #[error("{}: damaged archive ({reason})", path.display())]
    ArchiveDamaged { path: PathBuf, reason: String },
    /// The password given does not open the encrypted entry or archive.
    #[error("{}: the password is wrong", path.display())]
    PasswordWrong { path: PathBuf },
    /// Windows refused a symbolic link: it needs Developer Mode or administrator rights. A
    /// junction (directories) or a shortcut needs neither.
    #[error("{}: symbolic links need Developer Mode or administrator rights", path.display())]
    SymlinkPrivilege { path: PathBuf },
    /// No answer within the time limit: a share whose server is gone, a drive that does not
    /// respond ([`crate::TimeoutFs`]). Its code is `drive_timed_out`: `timed_out` is the MCP
    /// question nobody answered, and the window keys both texts on the code (RC-080-12).
    #[error("{}: no answer within {seconds} s", path.display())]
    TimedOut { path: PathBuf, seconds: u64 },
    /// The call was stopped while it waited for its answer ([`crate::IoCancel`]).
    #[error("{}: cancelled", path.display())]
    Cancelled { path: PathBuf },
}

impl VfsError {
    /// Every code [`VfsError::code`] returns, for what needs one entry per code (the i18n
    /// catalogue).
    pub const ALL_CODES: &[&str] = &[
        "not_found",
        "permission_denied",
        "already_exists",
        "not_a_directory",
        "is_a_directory",
        "directory_not_empty",
        "invalid_path",
        "unsupported",
        "io",
        "password_required",
        "archive_limit",
        "archive_decoder_memory",
        "archive_damaged",
        "password_wrong",
        "symlink_privilege",
        "drive_timed_out",
        "cancelled",
    ];

    /// Maps an I/O error on `path` to the variant that names its kind. An I/O error that wraps a
    /// `VfsError` (what a stream reports mid-read, see [`VfsError::into_io`]) gives that error
    /// back, its own path included.
    pub fn from_io(source: io::Error, path: impl Into<PathBuf>) -> Self {
        if source.get_ref().is_some_and(|inner| inner.is::<Self>()) {
            let inner = source
                .into_inner()
                .expect("checked: the error wraps a VfsError");
            return *inner
                .downcast::<Self>()
                .expect("checked: the error wraps a VfsError");
        }
        let path = path.into();
        match source.kind() {
            io::ErrorKind::NotFound => Self::NotFound { path },
            io::ErrorKind::PermissionDenied => Self::PermissionDenied { path },
            io::ErrorKind::AlreadyExists => Self::AlreadyExists { path },
            io::ErrorKind::NotADirectory => Self::NotADirectory { path },
            io::ErrorKind::IsADirectory => Self::IsADirectory { path },
            io::ErrorKind::DirectoryNotEmpty => Self::DirectoryNotEmpty { path },
            io::ErrorKind::Unsupported => Self::Unsupported {
                path,
                operation: "this operation",
            },
            _ => Self::Io { path, source },
        }
    }

    /// This error as an I/O error, for a stream that fails mid-read: [`VfsError::from_io`] gets
    /// it back intact, code and all.
    #[must_use]
    pub fn into_io(self) -> io::Error {
        let kind = match &self {
            Self::NotFound { .. } => io::ErrorKind::NotFound,
            Self::PermissionDenied { .. } => io::ErrorKind::PermissionDenied,
            Self::Unsupported { .. } => io::ErrorKind::Unsupported,
            Self::ArchiveDamaged { .. } => io::ErrorKind::InvalidData,
            Self::ArchiveDecoderMemory { .. } => io::ErrorKind::OutOfMemory,
            Self::TimedOut { .. } => io::ErrorKind::TimedOut,
            Self::Cancelled { .. } => io::ErrorKind::Interrupted,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, self)
    }

    /// The stable machine-readable code: what the UI and the MCP server key on.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::PermissionDenied { .. } => "permission_denied",
            Self::AlreadyExists { .. } => "already_exists",
            Self::NotADirectory { .. } => "not_a_directory",
            Self::IsADirectory { .. } => "is_a_directory",
            Self::DirectoryNotEmpty { .. } => "directory_not_empty",
            Self::InvalidPath { .. } => "invalid_path",
            Self::Unsupported { .. } => "unsupported",
            Self::Io { .. } => "io",
            Self::PasswordRequired { .. } => "password_required",
            Self::ArchiveLimit { .. } => "archive_limit",
            Self::ArchiveDecoderMemory { .. } => "archive_decoder_memory",
            Self::ArchiveDamaged { .. } => "archive_damaged",
            Self::PasswordWrong { .. } => "password_wrong",
            Self::SymlinkPrivilege { .. } => "symlink_privilege",
            Self::TimedOut { .. } => "drive_timed_out",
            Self::Cancelled { .. } => "cancelled",
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
