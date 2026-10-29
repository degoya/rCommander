//! The recycle bin on Windows, through the `trash` crate (the decision is in `trash.rs`).

use std::path::Path;

use crate::OpsError;

/// Moves `path` into the recycle bin, refusing network paths, whose "recycle bin" would delete
/// permanently without a word.
pub(super) fn trash(path: &Path) -> Result<(), OpsError> {
    let real = std::fs::canonicalize(rc_vfs::long_path(path))
        .map_err(|error| OpsError::from_io(error, path))?;
    if super::on_network(&real) {
        return Err(OpsError::TrashUnavailable {
            path: path.to_path_buf(),
        });
    }
    ::trash::delete(&real).map_err(|error| OpsError::TrashFailed {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })
}
