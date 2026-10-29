//! Compare by content (Total Commander's "Compare by content"): two files byte by byte, and
//! when both are text, line by line.
//!
//! The byte comparison streams ([`binary`]): two files of any size cost two buffers. The line
//! comparison ([`text`]) reads both files whole, so it runs only up to [`TEXT_DIFF_LIMIT`] per
//! file - and files that small are read once, whole, for both comparisons; larger files are
//! answered with the first differing byte alone. Both work on any
//! [`Vfs`], so local, remote and archive files compare alike.

pub mod binary;
pub mod text;

use std::path::Path;

use rc_vfs::{Vfs, VfsError, VfsPath};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt as _;
use tokio_util::sync::CancellationToken;

use crate::EngineError;
pub use binary::BinaryDifference;
pub use text::{
    DiffRow, Encoding, Line, LineEnding, LinePosition, RowKind, TextDiff, TextFile, TextOptions,
    Unified,
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
}

/// What a comparison of two files found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileComparison {
    pub left: VfsPath,
    pub right: VfsPath,
    pub left_size: u64,
    pub right_size: u64,
    /// The two hold the same bytes.
    pub identical: bool,
    /// The first byte that differs; `None` when identical.
    pub first_difference: Option<BinaryDifference>,
    /// The line comparison, when the bytes differ and both files are text within
    /// [`TEXT_DIFF_LIMIT`] and [`text::MAX_LINES`].
    pub text: Option<TextComparison>,
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
    let left_size = file_size(left).await?;
    let right_size = file_size(right).await?;
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
        identical: difference.is_none(),
        first_difference: difference,
        text,
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
    // The diff itself is CPU work of up to a few seconds: off the async workers.
    let options = *options;
    tokio::task::spawn_blocking(move || TextDiff::compute(left_text, right_text, &options))
        .await
        .map(Some)
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

/// The size of a file; a directory is refused.
async fn file_size(side: Side<'_>) -> Result<u64, EngineError> {
    let entry = side.vfs.stat(&side.path.path).await?;
    if entry.is_dir_like() {
        return Err(VfsError::IsADirectory {
            path: side.path.path.clone(),
        }
        .into());
    }
    Ok(entry.size)
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
