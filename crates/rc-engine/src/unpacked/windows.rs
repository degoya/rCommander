//! The Windows half of the unpacked copies' cleanup: whether a process still runs (through
//! `winsafe`'s safe wrappers) and the read-only attribute that would keep a copy in place.

use std::{fs, path::Path};

use winsafe::{self as w, co};

/// The exit code of a process that has not ended.
const STILL_ACTIVE: u32 = 259;

/// Whether the process `pid` is running. A process this user may not open (another user's,
/// a protected one) counts as running; only "no such process" or an exit code says it ended.
pub(super) fn alive(pid: u32) -> bool {
    match w::HPROCESS::OpenProcess(co::PROCESS::QUERY_LIMITED_INFORMATION, false, pid) {
        Ok(process) => !matches!(process.GetExitCodeProcess(), Ok(code) if code != STILL_ACTIVE),
        Err(error) => error != co::ERROR::INVALID_PARAMETER,
    }
}

/// Clears the read-only attribute of the plain files directly in `dir`, so the folder can be
/// removed; links are not followed.
#[allow(
    clippy::permissions_set_readonly_false,
    reason = "Windows: this clears the read-only attribute, there is no Unix mode to widen"
)]
pub(super) fn clear_read_only(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        if let Ok(metadata) = entry.metadata() {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                // A file that keeps its attribute keeps its folder until the next sweep.
                let _ = fs::set_permissions(entry.path(), permissions);
            }
        }
    }
}
