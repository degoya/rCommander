//! `copy_file_range` in pieces: between two the watch is looked at - the progress, a pause, a
//! cancel. A file system without it (or between two that cannot) answers at the first piece,
//! before a byte is written: `Unsupported`, and the caller streams.

use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

use nix::{errno::Errno, fcntl::copy_file_range};

use super::{cancelled, remove_unfinished};
use crate::CopyWatch;

/// The most one call copies: 4 MiB, a few milliseconds on a disk, so a pause or a cancel is
/// seen at once, and still whole extents to a reflink.
pub(super) const PIECE: usize = 4 << 20;

/// See [`super::copy`].
pub(super) fn copy(from: &Path, to: &Path, watch: &CopyWatch) -> io::Result<u64> {
    let source = File::open(from)?;
    // Made as a stream makes it (`LocalFs::open_write`): new, with the default mode.
    let target = OpenOptions::new().write(true).create_new(true).open(to)?;
    let copied = pieces(&source, &target, watch);
    if copied.is_err() {
        drop(target);
        remove_unfinished(to);
    }
    copied
}

/// Every piece of `source` into `target`, to the source's end.
fn pieces(source: &File, target: &File, watch: &CopyWatch) -> io::Result<u64> {
    let mut copied = 0_u64;
    loop {
        if !watch.go_on() {
            return Err(cancelled());
        }
        match copy_file_range(source, None, target, None, PIECE) {
            // Nothing at the start: a file system that copies nothing this way (a virtual one
            // whose files state no size) - or a file emptied since it was listed; the stream
            // copies either.
            Ok(0) if copied == 0 => return Err(io::ErrorKind::Unsupported.into()),
            Ok(0) => return Ok(copied),
            Ok(count) => {
                copied += count as u64;
                watch.report(copied);
            }
            Err(Errno::EINTR) => {}
            Err(errno) if copied == 0 && not_this_way(errno) => {
                return Err(io::ErrorKind::Unsupported.into());
            }
            Err(errno) => return Err(io::Error::from(errno)),
        }
    }
}

/// Whether `errno` at the first piece says "not this way" (as std's own copy reads it): no such
/// call, two file systems, a file system that refuses it.
fn not_this_way(errno: Errno) -> bool {
    matches!(
        errno,
        Errno::ENOSYS
            | Errno::EXDEV
            | Errno::EINVAL
            | Errno::EPERM
            | Errno::EOPNOTSUPP
            | Errno::EBADF
    )
}
