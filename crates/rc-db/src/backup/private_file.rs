//! A file a person chose, written the way a backup is (RC-080-19, the export of settings and
//! connections): never through a name that is already there, owner-only, on disk before it has
//! its name.

use std::{io::Write as _, path::Path};

use super::{BackupError, PartFile, publish};

/// `bytes` into the file `target`. They go to a temporary name beside it, created with
/// `create_new` - an existing file or symbolic link there is refused, never followed - and
/// owner-only on Unix (`0600`; on Windows the file inherits the ACL of its folder, in a profile
/// the person's own). The file is flushed to disk, then takes the target's name: replacing it
/// only with `overwrite` (a link at the target is replaced, not followed), never otherwise. On any
/// failure the temporary file goes and the target is untouched.
///
/// # Errors
///
/// [`BackupError::TargetExists`] without `overwrite`; [`BackupError::Target`] when the
/// temporary file cannot be made, written or renamed.
pub fn write_private_file(target: &Path, bytes: &[u8], overwrite: bool) -> Result<(), BackupError> {
    let failed = |source| BackupError::Target {
        path: target.to_path_buf(),
        source,
    };
    let (part, mut file) = PartFile::create_open(target)?;
    file.write_all(bytes).map_err(failed)?;
    file.sync_all().map_err(failed)?;
    drop(file);
    publish(&part.path, target, overwrite)
}

#[cfg(test)]
#[path = "private_file_tests.rs"]
mod tests;
