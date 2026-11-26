//! [`crate::Vfs::copy_file`] on this machine's disks (RC-110-65): the operating system's own
//! copy, under a [`CopyWatch`] that sees its bytes, holds it while paused and stops it.
//!
//! - **Windows**: `CopyFileExW` (`system_copy/windows.rs`). Within one SMB share the server copies
//!   the file itself and the client moves next to nothing; on a local disk the system's copy
//!   engine does it. Its progress routine reports, waits while paused and cancels
//!   (`PROGRESS_CANCEL`, which removes the target). `CopyFileExW` carries the source's attributes
//!   and modification time; the new file gets those of any new file again (the archive flag, the
//!   time of now), as one written through a stream has them - the caller sets what its job keeps.
//!   The file's other streams stay as `CopyFileExW` copies them, its Mark-of-the-Web among them.
//! - **Linux**: `copy_file_range` in pieces of [`linux::PIECE`] (`system_copy/linux.rs`): a
//!   reflink where the file system has one (Btrfs, XFS), a copy on the server on NFS 4.2 and SMB
//!   3, else a copy inside the kernel. The new file is made as a stream makes it.
//! - **Elsewhere** (macOS): `Unsupported`, and the caller streams the file. `clonefile` and
//!   `fcopyfile` have no wrapper in std or nix with progress and cancel, and `unsafe` is denied
//!   outside the Windows modules.
//!
//! Whatever fails, nothing is left at `to`: a target this call made and did not finish is
//! removed. `to` must not exist; one that does is `already_exists` and stays.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

use std::{io, path::Path};

use crate::CopyWatch;

/// Copies `from` to the new file `to`, both real paths ([`crate::long_path`]); the length of
/// `to` when done. `Unsupported` before anything was written where this file system or platform
/// has no such copy; `Interrupted` once `watch` cancelled it. Blocking.
pub(super) fn copy(from: &Path, to: &Path, watch: &CopyWatch) -> io::Result<u64> {
    #[cfg(target_os = "linux")]
    {
        linux::copy(from, to, watch)
    }
    #[cfg(windows)]
    {
        windows::copy(from, to, watch)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (from, to, watch);
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// The error of a copy `watch` stopped.
#[cfg(any(target_os = "linux", windows))]
fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "the copy was cancelled")
}

/// Removes `to`, the target a copy made and did not finish; one already gone is fine, a refusal
/// is logged - the copy's own error is the one that counts.
#[cfg(any(target_os = "linux", windows))]
fn remove_unfinished(to: &Path) {
    match std::fs::remove_file(to) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(
            path = %to.display(),
            %error,
            "the target of a system copy that did not finish could not be removed"
        ),
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "system_copy_tests.rs"]
mod tests;
