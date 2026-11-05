//! A restore's file through the handle that is checked, hashed and renamed (RC-080-68).
//!
//! The copy of a backup is written through the handle of its owner-only part file - never by
//! `fs::copy`, which gives the copy the source's permissions. Its digest is read through that
//! handle, and a name is taken only while it still names the handle's file: a file put under the
//! name in between - by a program with write access to the private folder - is never the one
//! hashed and swapped in.

use std::{
    fs,
    io::{self, SeekFrom},
    path::Path,
};

use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

/// Copies the regular file at `source` into `file`, replacing what it held, and writes it to the
/// disk.
///
/// # Errors
///
/// The source cannot be read or the copy written.
pub(super) async fn copy_into(source: &Path, file: &mut tokio::fs::File) -> io::Result<()> {
    let mut from = tokio::fs::File::open(source).await?;
    if !from.metadata().await?.is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    file.set_len(0).await?;
    file.seek(SeekFrom::Start(0)).await?;
    tokio::io::copy(&mut from, file).await?;
    file.sync_all().await
}

/// The BLAKE3 digest of `file`, read from its start.
///
/// # Errors
///
/// The file cannot be read.
pub(super) async fn digest(file: &mut tokio::fs::File) -> io::Result<blake3::Hash> {
    file.seek(SeekFrom::Start(0)).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

/// Fails unless `path` names `file` itself (not a link to it) and `file` still has the digest
/// `expected`: the file was neither replaced under its name nor changed through it.
///
/// # Errors
///
/// [`io::ErrorKind::InvalidData`] when it was; the file or the name cannot be read.
pub(super) async fn still(
    file: &mut tokio::fs::File,
    path: &Path,
    expected: blake3::Hash,
) -> io::Result<()> {
    let held = file.metadata().await?;
    let named = tokio::fs::symlink_metadata(path).await?;
    if !same_file(&held, &named) {
        return Err(changed(path, "another file took its name"));
    }
    // `blake3::Hash` compares in constant time.
    if digest(file).await? != expected {
        return Err(changed(path, "its contents changed"));
    }
    Ok(())
}

/// Why `path` is no longer the file that was checked, by its file name alone.
fn changed(path: &Path, what: &str) -> io::Error {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{name}: {what} after its check"),
    )
}

/// Whether two metadata describe the same file: device and inode on Unix.
#[cfg(unix)]
fn same_file(one: &fs::Metadata, other: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    one.dev() == other.dev() && one.ino() == other.ino()
}

/// Whether two metadata describe the same file. Windows has no stable file id in `std`: size,
/// creation and last write time, and a regular file - the digest that follows does the rest.
#[cfg(not(unix))]
fn same_file(one: &fs::Metadata, other: &fs::Metadata) -> bool {
    other.is_file()
        && one.len() == other.len()
        && one.created().ok() == other.created().ok()
        && one.modified().ok() == other.modified().ok()
}

#[cfg(test)]
#[path = "held_tests.rs"]
mod tests;
