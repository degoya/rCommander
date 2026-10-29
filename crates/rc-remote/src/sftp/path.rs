//! Backend paths as SFTP paths: absolute, `/`-separated, whatever the local platform writes. The
//! spelling is the shared one of `crate::wire`, with SFTP's own refusals.

use std::path::Path;

use rc_vfs::{Result, VfsError};

/// The FTP pool reads its `PWD` reply through it (`ftp/pool.rs`).
#[cfg(test)]
pub(crate) use crate::wire::from_wire;
use crate::wire::{Refusals, absolute};

/// What SFTP says when it refuses a path.
const REFUSALS: Refusals = Refusals {
    relative: "an SFTP path is absolute",
    prefix: "a drive or share is not an SFTP path",
    parent: "`..` is not resolved over SFTP",
};

/// The SFTP path (`/a/b`) of a backend path.
///
/// Relative paths, `..`, Windows prefixes, names that are not Unicode and names holding NUL
/// (which a C server would cut short) are invalid.
pub(crate) fn wire_path(path: &Path) -> Result<String> {
    absolute(path, &REFUSALS, |name| {
        if name.contains('\0') {
            return Err(VfsError::InvalidPath {
                path: path.to_path_buf(),
                reason: "a name cannot hold NUL",
            });
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "path_tests.rs"]
mod tests;
