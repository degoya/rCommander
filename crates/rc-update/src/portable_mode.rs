//! The portable mode (RC-100-95): a marker file beside the program, [`MARKER`], makes `data/`
//! beside it ([`DATA_DIR`]) the data folder instead of the one in the person's profile - a copy on
//! a USB stick that carries its settings, connections and plugins along.
//!
//! The marker is looked for in the program's own folder only, never in the working folder, and
//! its content does not matter (it is not read). It counts only where a portable copy can live:
//! [`check`] refuses
//!
//! * a package manager's installation (Scoop, winget, Homebrew, deb, rpm): its upgrade replaces
//!   the folder and would take the data with it;
//! * a folder this process cannot write (`C:\Program Files`): SQLite writes all the time, a
//!   read-only database does not carry a session;
//! * a network drive (UNC, or a mapped letter Windows calls remote): WAL is not reliable there;
//! * a `data` that is a link or junction: it would lead the data somewhere the folder does not
//!   show;
//! * macOS: a marker inside the signed `.app` breaks its signature, and App Translocation starts
//!   it from a read-only folder of chance.
//!
//! Whoever can write the program's folder can replace the program as well; to them the marker
//! opens no new way in. An MCP client is not among them (RC-110-19): a marker planted beside the
//! program would start the next run on an empty data folder - the MCP policy at its defaults, the
//! trusted host keys and the audit log gone - so `rc-mcp`'s write guard refuses the program's
//! folder and the marker's name anywhere. The update leaves [`MARKER`] and [`DATA_DIR`] where they are
//! (`install/portable.rs`, `KEPT`).

use std::path::{Component, Path, PathBuf, Prefix};

use crate::install_kind::{InstallKind, Installation, writable};

/// The marker file beside the program.
pub const MARKER: &str = "rcommander.portable";
/// The data folder of a portable copy, beside the program.
pub const DATA_DIR: &str = "data";
/// What the marker says to a person who opens it.
pub const MARKER_TEXT: &str = "rCommander keeps its data in the folder data beside this file. Delete this file to use the \
     data folder in your profile again.\n";

/// Every refusal of the portable mode, a code the window translates (`error.<code>`).
pub const REFUSAL_CODES: &[&str] = &[
    "update.portable_macos",
    "update.portable_package_manager",
    "update.portable_not_writable",
    "update.portable_network_drive",
    "update.portable_linked_data",
    "update.portable_database_exists",
    "update.portable_already",
];

/// Whether the marker lies beside the program in `folder`: a plain file, not a link.
#[must_use]
pub fn marker_present(folder: &Path) -> bool {
    std::fs::symlink_metadata(folder.join(MARKER)).is_ok_and(|meta| meta.is_file())
}

/// The portable data folder of `installation`, or the code of why it cannot have one.
///
/// # Errors
///
/// One of [`REFUSAL_CODES`]' first five.
pub fn check(installation: &Installation) -> Result<PathBuf, &'static str> {
    check_with(
        installation,
        cfg!(target_os = "macos"),
        on_network_drive(&installation.install_dir),
    )
}

/// [`check`] from explicit inputs, so every rule is testable on every platform.
pub(crate) fn check_with(
    installation: &Installation,
    macos: bool,
    network: bool,
) -> Result<PathBuf, &'static str> {
    if macos {
        return Err("update.portable_macos");
    }
    let folder = &installation.install_dir;
    match installation.kind {
        InstallKind::Portable => {}
        InstallKind::Unknown => {
            // The copy of `scripts/run-windows.sh` and a build from source carry no
            // `VERSION.txt`; their folder still has to take the data.
            if !writable(folder) {
                return Err("update.portable_not_writable");
            }
        }
        _ => return Err("update.portable_package_manager"),
    }
    if network {
        return Err("update.portable_network_drive");
    }
    let data = folder.join(DATA_DIR);
    if std::fs::symlink_metadata(&data).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("update.portable_linked_data");
    }
    Ok(data)
}

/// Whether `folder` is on a network share: a UNC path, or on Windows a drive letter the system
/// calls remote.
#[must_use]
pub fn on_network_drive(folder: &Path) -> bool {
    match folder.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) => true,
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => remote_letter(letter),
            _ => false,
        },
        _ => false,
    }
}

#[cfg(windows)]
fn remote_letter(letter: u8) -> bool {
    use winsafe::{self as w, co};
    let root = format!("{}:\\", char::from(letter));
    w::GetDriveType(Some(&root)) == co::DRIVE::REMOTE
}

/// Only Windows has drive letters.
#[cfg(not(windows))]
fn remote_letter(_letter: u8) -> bool {
    false
}

#[cfg(test)]
#[path = "portable_mode_tests.rs"]
mod tests;
