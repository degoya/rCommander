//! A whole file within a limit: the one bounded reader over a [`Vfs`] (RC-110-30).

use std::path::Path;

use tokio::io::AsyncReadExt as _;

use crate::{Result, Vfs, VfsError};

/// The whole file at `path`, or `None` when it holds more than `limit` bytes. Read through
/// `take`, so a file that grows while it is read is never held beyond the limit either.
///
/// # Errors
///
/// The file cannot be opened or read.
pub async fn read_within(vfs: &dyn Vfs, path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    let stream = vfs.open_read(path).await?;
    let mut bytes = Vec::new();
    stream
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| VfsError::from_io(error, path))?;
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
