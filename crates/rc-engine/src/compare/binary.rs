//! Byte-by-byte comparison of two streams, in chunks: two files of any size are compared with
//! two buffers of [`CHUNK`] bytes, and the comparison stops at the first difference.

use std::io;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio_util::sync::CancellationToken;

/// Bytes read from each stream at a time.
pub const CHUNK: usize = 256 * 1024;

/// Where two streams first differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryDifference {
    /// Bytes from the start that are the same in both.
    pub offset: u64,
    /// The left byte at `offset`; `None` when the left stream ended there.
    pub left: Option<u8>,
    /// The right byte at `offset`; `None` when the right stream ended there.
    pub right: Option<u8>,
}

/// Why a comparison of two streams stopped without an answer.
#[derive(Debug)]
pub enum ReadFailure {
    /// Reading the left stream failed.
    Left(io::Error),
    /// Reading the right stream failed.
    Right(io::Error),
    /// The caller cancelled.
    Cancelled,
}

/// The first difference of `left` and `right`, or `None` when they hold the same bytes. Reads
/// no further than the first difference; checks `cancel` before every chunk.
///
/// # Errors
///
/// A read failed, or `cancel` was cancelled.
pub async fn first_difference<L, R>(
    left: &mut L,
    right: &mut R,
    cancel: &CancellationToken,
) -> Result<Option<BinaryDifference>, ReadFailure>
where
    L: AsyncRead + Unpin + ?Sized,
    R: AsyncRead + Unpin + ?Sized,
{
    let mut left_buf = vec![0_u8; CHUNK];
    let mut right_buf = vec![0_u8; CHUNK];
    // The unread parts of the buffers: `start..end`. A stream may answer fewer bytes than asked,
    // and the two seldom answer the same number, so each refills only when it is used up.
    let (mut left_start, mut left_end) = (0, 0);
    let (mut right_start, mut right_end) = (0, 0);
    let mut offset = 0_u64;
    loop {
        if cancel.is_cancelled() {
            return Err(ReadFailure::Cancelled);
        }
        if left_start == left_end {
            left_start = 0;
            left_end = left.read(&mut left_buf).await.map_err(ReadFailure::Left)?;
        }
        if right_start == right_end {
            right_start = 0;
            right_end = right
                .read(&mut right_buf)
                .await
                .map_err(ReadFailure::Right)?;
        }
        let left_part = &left_buf[left_start..left_end];
        let right_part = &right_buf[right_start..right_end];
        if left_part.is_empty() || right_part.is_empty() {
            return Ok((!left_part.is_empty() || !right_part.is_empty()).then(|| {
                BinaryDifference {
                    offset,
                    left: left_part.first().copied(),
                    right: right_part.first().copied(),
                }
            }));
        }
        let len = left_part.len().min(right_part.len());
        if let Some(index) = differs_at(&left_part[..len], &right_part[..len]) {
            return Ok(Some(BinaryDifference {
                offset: offset + index as u64,
                left: left_part.get(index).copied(),
                right: right_part.get(index).copied(),
            }));
        }
        left_start += len;
        right_start += len;
        offset += len as u64;
    }
}

/// [`first_difference`] of two files read whole: what a comparison of small files has in hand
/// anyway.
#[must_use]
pub fn first_difference_of(left: &[u8], right: &[u8]) -> Option<BinaryDifference> {
    let len = left.len().min(right.len());
    let index = match differs_at(&left[..len], &right[..len]) {
        Some(index) => index,
        None if left.len() == right.len() => return None,
        // One holds the other and more.
        None => len,
    };
    Some(BinaryDifference {
        offset: index as u64,
        left: left.get(index).copied(),
        right: right.get(index).copied(),
    })
}

/// Where two parts of one length first differ. The slice comparison is a `memcmp`; the byte
/// loop runs only once they differ.
fn differs_at(left: &[u8], right: &[u8]) -> Option<usize> {
    if left == right {
        return None;
    }
    left.iter().zip(right).position(|(a, b)| a != b)
}

#[cfg(test)]
#[path = "binary_tests.rs"]
mod tests;
