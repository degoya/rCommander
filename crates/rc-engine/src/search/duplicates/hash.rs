//! Reading a file for the duplicate search: the BLAKE3 digest of its first bytes or of all of
//! it, through one reused buffer, stopped at the next chunk once the search is cancelled.

use std::path::Path;

use rc_checksum::Algorithm;
use rc_vfs::{Vfs, VfsError};
use tokio::io::AsyncReadExt as _;

use crate::search::SearchCancel;

/// The bytes compared first: files alike in these are read whole.
pub(super) const HEAD: u64 = 64 * 1024;

/// The bytes read at once.
const CHUNK: usize = 64 * 1024;

/// Why a file has no digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    /// The search was cancelled.
    Cancelled,
    /// The file could not be read: the error's code.
    Failed(&'static str),
}

/// The digest of the first `limit` bytes of the file at `path` (all of it for `None`), as hex.
///
/// # Errors
///
/// [`Stop::Cancelled`] once `cancel` is set, [`Stop::Failed`] when the file cannot be read.
pub(super) async fn digest(
    vfs: &dyn Vfs,
    path: &Path,
    limit: Option<u64>,
    buf: &mut Vec<u8>,
    cancel: &SearchCancel,
) -> Result<String, Stop> {
    if cancel.is_cancelled() {
        return Err(Stop::Cancelled);
    }
    let failed = |error: VfsError| Stop::Failed(error.code());
    let stream = vfs.open_read(path).await.map_err(failed)?;
    let mut reader = stream.take(limit.unwrap_or(u64::MAX));
    buf.resize(CHUNK, 0);
    let mut hasher = Algorithm::Blake3.hasher();
    loop {
        if cancel.is_cancelled() {
            return Err(Stop::Cancelled);
        }
        let read = reader
            .read(&mut buf[..])
            .await
            .map_err(|error| failed(VfsError::from_io(error, path)))?;
        if read == 0 {
            return Ok(hasher.finish_hex());
        }
        hasher.update(&buf[..read]);
    }
}
