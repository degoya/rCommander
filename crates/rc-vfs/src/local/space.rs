//! The space of the file system a path is on, in bytes: one call for the drive bar
//! (`rc-engine`'s `drives.rs`) and [`crate::Vfs::free_space`] (RC-080-31).
//!
//! Windows asks `GetDiskFreeSpaceEx` through `winsafe`'s safe wrapper, Unix `statvfs` through
//! `nix`; no `unsafe` of ours. The available bytes are the caller's, quotas honoured, as
//! Explorer and `df` show them.

use std::{io, path::Path};

/// The space of the file system `path` is on: `(available, total)` in bytes, the available
/// bytes those the caller may still use. `path` is any directory of that file system, with or
/// without the `\\?\` prefix on Windows.
///
/// # Errors
///
/// The system's error when `path` does not exist or the drive does not answer (a sleeping
/// disk, a disconnected share); the caller bounds the wait.
pub fn disk_space(path: &Path) -> io::Result<(u64, u64)> {
    platform(path)
}

#[cfg(windows)]
fn platform(path: &Path) -> io::Result<(u64, u64)> {
    let (mut available, mut total) = (0u64, 0u64);
    winsafe::GetDiskFreeSpaceEx(
        Some(&path.to_string_lossy()),
        Some(&mut available),
        Some(&mut total),
        None,
    )
    .map_err(|error| io::Error::from_raw_os_error(u32::from(error) as i32))?;
    Ok((available, total))
}

#[cfg(unix)]
fn platform(path: &Path) -> io::Result<(u64, u64)> {
    let stats = nix::sys::statvfs::statvfs(path).map_err(io::Error::from)?;
    let fragment = widen(stats.fragment_size());
    Ok((
        fragment.saturating_mul(widen(stats.blocks_available())),
        fragment.saturating_mul(widen(stats.blocks())),
    ))
}

/// A block count or size as `u64`: the C types are 32 bits wide on some targets.
#[cfg(unix)]
fn widen<T: Into<u64>>(value: T) -> u64 {
    value.into()
}

#[cfg(not(any(windows, unix)))]
fn platform(_path: &Path) -> io::Result<(u64, u64)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no disk space call on this platform",
    ))
}

#[cfg(test)]
#[path = "space_tests.rs"]
mod tests;
