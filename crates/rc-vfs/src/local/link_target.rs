//! Which links a listing does not follow to describe them (RC-080-10, RC-110-66).
//!
//! A listing describes a symbolic link by what it leads to: its size, times and kind. Following
//! a link onto a share makes Windows sign in to the share's server with the person's account, by
//! merely listing the folder (RC-080-10); following one onto a drive letter mapped to a share
//! whose server is gone holds the listing for as long as Windows waits for it, once per link
//! (RC-110-66). Neither is followed: the link is shown as a link whose target is unknown and is
//! looked up when it is used. `GetDriveTypeW` reads a letter's kind without touching the drive,
//! so asking it costs no wait of its own.

use std::path::Path;

use crate::{drive_letter, network_path};

/// Whether a link whose stored target is `target` is described without following it.
pub(super) fn not_followed(target: &Path) -> bool {
    judged(&target.to_string_lossy(), cfg!(windows), remote_drive)
}

/// [`not_followed`] for `target` as text, on Windows when `windows`, with `remote` saying
/// whether a drive letter is mapped to a share. Only Windows reaches another machine through a
/// link's target; elsewhere every target is followed.
fn judged(target: &str, windows: bool, remote: impl Fn(char) -> bool) -> bool {
    if !windows {
        return false;
    }
    if network_path(target).is_some() {
        return true;
    }
    let plain = target.strip_prefix(r"\\?\").unwrap_or(target);
    drive_letter(plain).is_some_and(remote)
}

/// Whether `letter` is a drive mapped to a share (`DRIVE_REMOTE`), connected or not.
#[cfg(windows)]
fn remote_drive(letter: char) -> bool {
    use winsafe::{self as w, co};
    let root = format!("{letter}:\\");
    w::GetDriveType(Some(&root)) == co::DRIVE::REMOTE
}

/// Only Windows has drive letters.
#[cfg(not(windows))]
fn remote_drive(_letter: char) -> bool {
    false
}

#[cfg(test)]
#[path = "link_target_tests.rs"]
mod tests;
