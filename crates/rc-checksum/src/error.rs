//! The errors of hashing, checksum files, split and combine, each with a stable code.

use std::{io, path::Path, path::PathBuf};

use rc_vfs::VfsError;

/// What went wrong. [`ChecksumError::code`] is the stable form the GUI keys its message on and
/// the MCP server answers with; a file-system failure keeps its [`VfsError`] code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ChecksumError {
    /// The file system refused or failed.
    #[error(transparent)]
    Vfs(#[from] VfsError),
    /// The run was cancelled; what it had written is removed.
    #[error("cancelled")]
    Cancelled,
    /// A name given for an entry is empty, a path, `.` or `..`.
    #[error("{name:?} is not a file name")]
    InvalidName { name: String },
    /// The selection holds no file to hash.
    #[error("no file to hash")]
    NoFiles,
    /// The entry is neither a file nor a folder (a socket, a device, a dangling link).
    #[error("{}: not a regular file", path.display())]
    NotAFile { path: PathBuf },
    /// A checksum file larger than [`crate::MAX_CHECKSUM_FILE`] is not read.
    #[error("{}: too large for a checksum file ({size} bytes)", path.display())]
    ChecksumFileTooLarge { path: PathBuf, size: u64 },
    /// No line of the file is a checksum line of a known format.
    #[error("{}: not a checksum file of a known format", path.display())]
    UnknownFormat { path: PathBuf },
    /// A split `.crc` file is malformed.
    #[error("{}: not a valid split .crc file ({reason})", path.display())]
    InvalidCrcFile { path: PathBuf, reason: &'static str },
    /// A part size of 0.
    #[error("the part size must be at least one byte")]
    InvalidPartSize,
    /// The split would need more parts than three digits number.
    #[error("{parts} parts are too many; at most {max} - choose a larger part size")]
    TooManyParts { parts: u64, max: u64 },
    /// The file to combine from is neither a `.001` part nor a `.crc` file.
    #[error("{}: not the first part (.001) or the .crc file of a split file", path.display())]
    NotASplitFile { path: PathBuf },
    /// A part of a split file is missing; nothing was written.
    #[error("part {name} is missing; nothing was combined")]
    PartMissing { name: String },
    /// The parts do not add up to the size the `.crc` file names; nothing was written.
    #[error("the parts hold {actual} bytes, the .crc file names {expected}; nothing was combined")]
    SizeMismatch { expected: u64, actual: u64 },
    /// The combined file's CRC differs from the `.crc` file's; the file was removed.
    #[error(
        "the combined file's CRC32 is {actual}, the .crc file names {expected}; it was removed"
    )]
    CrcMismatch { expected: String, actual: String },
    /// The run's guard keeps the file's contents from its caller ([`crate::Control::with_guard`]);
    /// nothing of it was read.
    #[error("{} is {reason}: this run may not read it", path.display())]
    Sensitive { path: PathBuf, reason: &'static str },
}

impl ChecksumError {
    /// Every code of this crate's own variants; a file-system failure carries one of
    /// [`VfsError::ALL_CODES`] instead.
    pub const ALL_CODES: &[&str] = &[
        "cancelled",
        "invalid_name",
        "checksum_no_files",
        "checksum_not_a_file",
        "checksum_file_too_large",
        "checksum_format_unknown",
        "split_crc_file_invalid",
        "split_part_size_invalid",
        "split_too_many_parts",
        "combine_not_a_split_file",
        "combine_part_missing",
        "combine_size_mismatch",
        "combine_crc_mismatch",
        "sensitive_path",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Vfs(error) => error.code(),
            Self::Cancelled => "cancelled",
            Self::InvalidName { .. } => "invalid_name",
            Self::NoFiles => "checksum_no_files",
            Self::NotAFile { .. } => "checksum_not_a_file",
            Self::ChecksumFileTooLarge { .. } => "checksum_file_too_large",
            Self::UnknownFormat { .. } => "checksum_format_unknown",
            Self::InvalidCrcFile { .. } => "split_crc_file_invalid",
            Self::InvalidPartSize => "split_part_size_invalid",
            Self::TooManyParts { .. } => "split_too_many_parts",
            Self::NotASplitFile { .. } => "combine_not_a_split_file",
            Self::PartMissing { .. } => "combine_part_missing",
            Self::SizeMismatch { .. } => "combine_size_mismatch",
            Self::CrcMismatch { .. } => "combine_crc_mismatch",
            Self::Sensitive { .. } => "sensitive_path",
        }
    }

    /// A failed read or write of an open stream on `path`.
    pub(crate) fn io(path: &Path, source: io::Error) -> Self {
        Self::Vfs(VfsError::from_io(source, path))
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
