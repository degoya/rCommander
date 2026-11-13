//! Blocking file-system work off the async threads: the one helper of the local and the archive
//! backend (RC-110-30).

use std::{io, path::Path};

use crate::recover::LogPanic as _;
use crate::{Result, VfsError};

/// Runs `work` on the blocking pool. A panic in it is logged under `task` and comes back as an
/// I/O error on `path`.
pub(crate) async fn blocking<T, F>(task: &'static str, path: &Path, work: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .log_panic(task)
        .map_err(|join| VfsError::Io {
            path: path.to_path_buf(),
            source: io::Error::other(join),
        })?
}

#[cfg(test)]
#[path = "work_tests.rs"]
mod tests;
