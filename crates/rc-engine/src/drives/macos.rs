//! macOS's network shares (RC-110-40): mounting one through the system, unmounting it, and the
//! mount table that tells a share's volume from a local one.
//!
//! Mounting runs AppleScript's `mount volume` in `osascript`: the system asks for the user name
//! and the password in its own dialog, offers to keep them in the keychain and mounts the share
//! under `/Volumes`; rCommander never sees them. The address is the script's argument, never part
//! of its text, so nothing in it is read as AppleScript. The call blocks until the person
//! answered; its error number (`-128` for the closed dialog) is the last thing osascript says.
//! Finder's `open smb://...` was the other way: it answers before anything is mounted and tells
//! no error. NetFS's `NetFSMountURLSync` has no safe binding, and `unsafe` is for Win32 only.
//!
//! Unmounting is `unmount(2)` (`nix`, no process): its errno tells open files (`EBUSY`) apart,
//! where `diskutil unmount` only prints a sentence; `force` is `MNT_FORCE`. The mount table is
//! `/sbin/mount`'s answer, which reads the system's table without waiting on any volume (a
//! `statfs` on a share whose server is gone can hang).

use std::{
    path::Path,
    process::{Command, Stdio},
};

use nix::mount::{MntFlags, unmount as unmount_at};
use rc_process::NoConsoleWindow as _;

use super::share::{self, MountEntry};

const OSASCRIPT: &str = "/usr/bin/osascript";
const MOUNT: &str = "/sbin/mount";

/// The script that mounts the address it is given as its first argument.
const SCRIPT: [&str; 3] = ["on run argv", "mount volume (item 1 of argv)", "end run"];

/// What [`mount`] answers when osascript did not start or named no error.
const NO_ANSWER: i32 = 0;

/// Mounts the share at `url` (`smb://server/share`), the system asking for the credentials;
/// blocks until the person answered. The error is AppleScript's number.
pub(super) fn mount(url: &str) -> Result<(), i32> {
    let mut command = Command::new(OSASCRIPT);
    for line in SCRIPT {
        command.arg("-e").arg(line);
    }
    let output = command
        .arg(url)
        .stdin(Stdio::null())
        .no_console_window()
        .output()
        .map_err(|error| {
            tracing::warn!(%error, "osascript did not start");
            NO_ANSWER
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = share::script_error(&stderr);
    tracing::debug!(status = %output.status, ?code, "mount volume refused");
    Err(code.unwrap_or(NO_ANSWER))
}

/// Unmounts the volume at `point`; without `force` refused while files on it are open. The error
/// is the errno.
pub(super) fn unmount(point: &Path, force: bool) -> Result<(), i32> {
    let flags = if force {
        MntFlags::MNT_FORCE
    } else {
        MntFlags::empty()
    };
    unmount_at(point, flags).map_err(|errno| errno as i32)
}

/// The system's mount table; empty when `mount` does not answer.
pub(super) fn mount_table() -> Vec<MountEntry> {
    match Command::new(MOUNT)
        .stdin(Stdio::null())
        .no_console_window()
        .output()
    {
        Ok(output) if output.status.success() => {
            share::parse_table(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(output) => {
            tracing::debug!(status = %output.status, "mount answered no table");
            Vec::new()
        }
        Err(error) => {
            tracing::debug!(%error, "mount did not start");
            Vec::new()
        }
    }
}
