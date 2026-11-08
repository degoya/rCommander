//! Hands a copy to its program without waiting: the program of the setting, or the system's
//! program for the file. The tests start nothing; they read what would have started.

use std::path::Path;

#[cfg(test)]
use crate::recover::LockOrRecover as _;

/// Starts `program` with `file`, or the system's program for it. The `open` crate passes the
/// path as one argument and starts no console window.
#[cfg(not(test))]
pub(super) fn launch(file: &Path, program: Option<&str>) -> std::io::Result<()> {
    match program {
        Some(program) => open::with_detached(file, program),
        None => open::that_detached(file),
    }
}

/// What the tests' launches would have started: the copy and the program.
#[cfg(test)]
pub(crate) static LAUNCHED: std::sync::Mutex<Vec<(std::path::PathBuf, Option<String>)>> =
    std::sync::Mutex::new(Vec::new());

/// The tests start nothing: the copy and the program are recorded.
#[cfg(test)]
pub(super) fn launch(file: &Path, program: Option<&str>) -> std::io::Result<()> {
    LAUNCHED
        .lock_or_recover("remote_edit.launched")
        .push((file.to_path_buf(), program.map(str::to_owned)));
    Ok(())
}
