//! Compare by content (Total Commander's "Compare by content"): two files byte by byte, and
//! when both are text, line by line.
//!
//! The byte comparison streams ([`binary`]): two files of any size cost two buffers. The line
//! comparison ([`text`]) reads both files whole, so it runs only up to [`TEXT_DIFF_LIMIT`] per
//! file - and files that small are read once, whole, for both comparisons; larger files are
//! answered with the first differing byte alone. Both work on any
//! [`Vfs`], so local, remote and archive files compare alike.
//!
//! The edit mode (RC-100-94) changes two text files line by line ([`edit`]), reads and writes
//! them back ([`save`]); the external compare tool's arguments are [`external`]. A compare plugin
//! compares two files by structure beside the line diff ([`structure`], RC-100-41).

pub mod binary;
pub mod edit;
pub mod external;
pub mod save;
pub mod structure;
pub mod text;

use std::path::Path;

use rc_vfs::{Entry, Vfs, VfsError, VfsPath, unix_seconds};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt as _;
use tokio_util::sync::CancellationToken;

use crate::EngineError;
use crate::recover::LogPanic as _;
pub use binary::BinaryDifference;
pub use edit::{CompareSide, Direction, EditSession, Origin, ReadOnly};
pub use structure::StructureOutcome;
pub use text::{
    DiffRow, Encoding, Line, LineEnding, LinePosition, RowKind, TextDiff, TextFile, TextOptions,
    Unified, Unmappable,
};

/// The largest file, in bytes, that is compared line by line.
pub const TEXT_DIFF_LIMIT: u64 = 16 * 1024 * 1024;

/// The largest unified diff, in bytes, a `compare.files` answer carries.
pub const UNIFIED_LIMIT_MAX: usize = 1024 * 1024;

/// Equal lines shown around each change of a unified diff.
const UNIFIED_CONTEXT: usize = 3;

/// A file on its file system.
#[derive(Clone, Copy)]
pub struct Side<'a> {
    pub vfs: &'a dyn Vfs,
    pub path: &'a VfsPath,
}

/// The parameters of `compare.files`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileCompareRequest {
    /// The left file: absolute, or relative to the left panel's directory. With `right`, both
    /// `None` compare the two selected files of the active panel, or else the files under the
    /// cursors of both panels.
    pub left: Option<String>,
    /// The right file: absolute, or relative to the right panel's directory.
    pub right: Option<String>,
    pub options: TextOptions,
    /// Bytes of unified diff to include, at most [`UNIFIED_LIMIT_MAX`]; none when 0.
    pub unified_limit: usize,
    /// Compare by structure as well, when a compare plugin claims both files (RC-100-41).
    pub structure: bool,
}

/// The parameters of `compare.by_content` (RC-100-94): the external compare tool when one is
/// set, else the diff window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ByContentRequest {
    /// The two files; both `None` picks them as `compare.files` does.
    pub left: Option<VfsPath>,
    pub right: Option<VfsPath>,
}

/// How a file is expected to be: its size and time as a `compare.files` answer reported them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileStamp {
    pub size: u64,
    pub modified_unix: Option<u64>,
}

/// The parameters of `compare.apply_blocks` (RC-100-94).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyBlocksRequest {
    /// The left file: absolute, or relative to the left panel's directory.
    pub left: String,
    /// The right file: absolute, or relative to the right panel's directory.
    pub right: String,
    /// The options the blocks were counted under.
    pub options: TextOptions,
    /// The blocks of differing lines to copy, from 0 in the order of the comparison.
    pub blocks: Vec<usize>,
    pub direction: Direction,
    /// Both files as the comparison found them; either changed since, nothing is written.
    pub expect_left: FileStamp,
    pub expect_right: FileStamp,
}

/// What a comparison of two files found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileComparison {
    pub left: VfsPath,
    pub right: VfsPath,
    pub left_size: u64,
    pub right_size: u64,
    /// When each was last changed, in Unix seconds, where the file system tells: with the sizes
    /// what `compare.apply_blocks` expects to find (RC-100-94).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_modified_unix: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_modified_unix: Option<u64>,
    /// The two hold the same bytes.
    pub identical: bool,
    /// The first byte that differs; `None` when identical.
    pub first_difference: Option<BinaryDifference>,
    /// The line comparison, when the bytes differ and both files are text within
    /// [`TEXT_DIFF_LIMIT`] and [`text::MAX_LINES`].
    pub text: Option<TextComparison>,
    /// The structural comparison, when the request asked for one (RC-100-41).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure: Option<StructureOutcome>,
}

/// The line comparison of two differing text files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextComparison {
    pub left_encoding: Encoding,
    pub right_encoding: Encoding,
    /// No line differs under the options (the bytes still do).
    pub equal: bool,
    /// Blocks of differing lines.
    pub changes: usize,
    /// Where the first block starts.
    pub first_change: Option<LinePosition>,
    /// The unified diff, when one was asked for.
    pub unified: Option<Unified>,
}

/// Compares two files: byte by byte, then - when they differ and both are small enough text -
/// line by line under `options`, with a unified diff of up to `unified_limit` bytes. Files of at
/// most [`TEXT_DIFF_LIMIT`] are read once, whole, for both; larger ones are streamed.
///
/// # Errors
///
/// Either path is a directory or cannot be read, or `cancel` was cancelled.
pub async fn compare_files(
    left: Side<'_>,
    right: Side<'_>,
    options: &TextOptions,
    unified_limit: usize,
    cancel: &CancellationToken,
) -> Result<FileComparison, EngineError> {
    let left_entry = file_entry(left).await?;
    let right_entry = file_entry(right).await?;
    let (left_size, right_size) = (left_entry.size, right_entry.size);
    let whole = if left_size <= TEXT_DIFF_LIMIT && right_size <= TEXT_DIFF_LIMIT {
        match (
            read_bounded(left, cancel).await?,
            read_bounded(right, cancel).await?,
        ) {
            (Some(left_bytes), Some(right_bytes)) => Some((left_bytes, right_bytes)),
            // One grew past the limit since: streamed as a large one is.
            _ => None,
        }
    } else {
        None
    };
    let (difference, text) = match whole {
        Some((left_bytes, right_bytes)) => {
            let difference = binary::first_difference_of(&left_bytes, &right_bytes);
            let text = match difference {
                Some(_) => diff_bytes(left_bytes, right_bytes, options)
                    .await?
                    .map(|diff| summarize(&diff, unified_limit.min(UNIFIED_LIMIT_MAX))),
                None => None,
            };
            (difference, text)
        }
        None => {
            let mut left_stream = left.vfs.open_read(&left.path.path).await?;
            let mut right_stream = right.vfs.open_read(&right.path.path).await?;
            let difference = binary::first_difference(&mut left_stream, &mut right_stream, cancel)
                .await
                .map_err(|failure| read_failure(failure, left.path, right.path))?;
            (difference, None)
        }
    };
    Ok(FileComparison {
        left: left.path.clone(),
        right: right.path.clone(),
        left_size,
        right_size,
        left_modified_unix: left_entry.modified.and_then(unix_seconds),
        right_modified_unix: right_entry.modified.and_then(unix_seconds),
        identical: difference.is_none(),
        first_difference: difference,
        text,
        structure: None,
    })
}

/// The side-by-side line diff of two files, or `None` when either is binary, larger than
/// [`TEXT_DIFF_LIMIT`] or holds [`text::MAX_LINES`] lines: what the diff window shows.
///
/// # Errors
///
/// Either file cannot be read, or `cancel` was cancelled.
pub async fn diff_files(
    left: Side<'_>,
    right: Side<'_>,
    options: &TextOptions,
    cancel: &CancellationToken,
) -> Result<Option<TextDiff>, EngineError> {
    let Some(left_bytes) = read_bounded(left, cancel).await? else {
        return Ok(None);
    };
    let Some(right_bytes) = read_bounded(right, cancel).await? else {
        return Ok(None);
    };
    diff_bytes(left_bytes, right_bytes, options).await
}

/// The line diff of two files read whole; `None` when either is no text.
async fn diff_bytes(
    left_bytes: Vec<u8>,
    right_bytes: Vec<u8>,
    options: &TextOptions,
) -> Result<Option<TextDiff>, EngineError> {
    let (Some(left_text), Some(right_text)) =
        (text::decode(&left_bytes), text::decode(&right_bytes))
    else {
        return Ok(None);
    };
    drop((left_bytes, right_bytes));
    diff_texts(left_text, right_text, options).await.map(Some)
}

/// The line diff of two texts, computed off the async workers: CPU work of up to a few seconds.
///
/// # Errors
///
/// The runtime shut down under it.
pub async fn diff_texts(
    left: TextFile,
    right: TextFile,
    options: &TextOptions,
) -> Result<TextDiff, EngineError> {
    let options = *options;
    tokio::task::spawn_blocking(move || TextDiff::compute(left, right, &options))
        .await
        .log_panic("engine.compare_diff")
        .map_err(|error| {
            if error.is_panic() {
                std::panic::resume_unwind(error.into_panic());
            }
            EngineError::Cancelled
        })
}

fn summarize(diff: &TextDiff, unified_limit: usize) -> TextComparison {
    TextComparison {
        left_encoding: diff.left.encoding,
        right_encoding: diff.right.encoding,
        equal: diff.is_equal(),
        changes: diff.changes(),
        first_change: diff.next_change(None).map(|row| diff.position(row)),
        unified: (unified_limit > 0).then(|| diff.unified(UNIFIED_CONTEXT, unified_limit)),
    }
}

/// A file's entry; a directory is refused.
async fn file_entry(side: Side<'_>) -> Result<Entry, EngineError> {
    let entry = side.vfs.stat(&side.path.path).await?;
    if entry.is_dir_like() {
        return Err(VfsError::IsADirectory {
            path: side.path.path.clone(),
        }
        .into());
    }
    Ok(entry)
}

/// A whole file, or `None` when it is larger than [`TEXT_DIFF_LIMIT`].
async fn read_bounded(
    side: Side<'_>,
    cancel: &CancellationToken,
) -> Result<Option<Vec<u8>>, EngineError> {
    if cancel.is_cancelled() {
        return Err(EngineError::Cancelled);
    }
    let stream = side.vfs.open_read(&side.path.path).await?;
    let mut bytes = Vec::new();
    stream
        .take(TEXT_DIFF_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| VfsError::from_io(error, &side.path.path))?;
    Ok((bytes.len() as u64 <= TEXT_DIFF_LIMIT).then_some(bytes))
}

pub(crate) fn read_failure(
    failure: binary::ReadFailure,
    left: &VfsPath,
    right: &VfsPath,
) -> EngineError {
    let io = |error, path: &Path| EngineError::from(VfsError::from_io(error, path));
    match failure {
        binary::ReadFailure::Left(error) => io(error, &left.path),
        binary::ReadFailure::Right(error) => io(error, &right.path),
        binary::ReadFailure::Cancelled => EngineError::Cancelled,
    }
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
