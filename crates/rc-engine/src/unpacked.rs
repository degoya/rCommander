//! The folders a file opened from inside an archive is unpacked into (RC-030-02): one per file,
//! `<root>/open-<pid>-<random>`, named after the process that made it and created by `tempfile`,
//! so no other process can guess the name first (RC-080-51).
//!
//! The root is the user's own (RC-080-51): on Windows `%TEMP%\rcommander` (the user's
//! temporary folder under `%LOCALAPPDATA%`), elsewhere `rcommander/unpacked` in the user's
//! runtime folder (`$XDG_RUNTIME_DIR`), else in the user's cache folder - never the shared
//! `/tmp` - kept at mode 0700.
//!
//! The app removes its own at exit ([`remove_own`]) and, at start, those of processes that have
//! ended ([`remove_stale`]) - a copy a program still holds open stays until a later start. Only
//! real folders named exactly `open-<digits>-<letters and digits>` are touched: no file, link or
//! junction, and nothing else under the root. The copies of server files being edited live under
//! the same root as `edit-<pid>-<random>` (RC-110-01, `remote_edit/folder.rs`): their own module
//! removes them, and the sweep at start only those of ended processes without a session file -
//! a folder with one holds a change kept at quit, which the next start offers.
//!
//! The list files of an own command's `%L`, `%UL`, ... (RC-100-86) live beside them in folders
//! `list-<pid>-<random>` ([`new_list_folder`]) and go the same way: a program that hands its list
//! to a running instance and returns at once must still find it, so it stays until rCommander
//! ends or starts again.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::recover::LogPanic as _;

#[cfg(windows)]
mod windows;

/// The folder all unpacked copies live under; `None` without a folder of the user's own.
#[cfg(windows)]
pub(crate) fn root() -> Option<PathBuf> {
    Some(std::env::temp_dir().join("rcommander"))
}

/// The folder all unpacked copies live under; `None` without a folder of the user's own.
#[cfg(not(windows))]
pub(crate) fn root() -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    let base = dirs.runtime_dir().unwrap_or_else(|| dirs.cache_dir());
    Some(base.join("rcommander").join("unpacked"))
}

/// Creates `root` when it is missing, as a folder of this user's alone.
#[cfg(windows)]
pub(crate) fn own_root(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)
}

/// Creates `root` when it is missing, as a folder of this user's alone: mode 0700, and a real
/// folder, not a link to one elsewhere.
#[cfg(not(windows))]
pub(crate) fn own_root(root: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};

    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(root)?;
    let found = fs::symlink_metadata(root)?;
    if !found.file_type().is_dir() {
        return Err(io::Error::other("the unpack folder is not a folder"));
    }
    if found.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Creates the folder for one unpacked file: under the root, with a name nobody guesses. Off the
/// async threads.
///
/// # Errors
///
/// No folder of the user's own to unpack into, or creating it fails.
pub(crate) async fn new_folder() -> io::Result<PathBuf> {
    tokio::task::spawn_blocking(|| create_folder("open"))
        .await
        .log_panic("engine.unpack_folder")
        .map_err(io::Error::other)?
}

/// Creates the folder for the list files of one start of an own command (RC-100-86), as
/// [`new_folder`] does for an unpacked file. Off the async threads.
///
/// # Errors
///
/// No folder of the user's own, or creating it fails.
pub(crate) async fn new_list_folder() -> io::Result<PathBuf> {
    tokio::task::spawn_blocking(|| create_folder("list"))
        .await
        .log_panic("engine.list_folder")
        .map_err(io::Error::other)?
}

/// [`new_folder`] or [`new_list_folder`], blocking: a folder `<kind>-<pid>-<random>`.
fn create_folder(kind: &str) -> io::Result<PathBuf> {
    let root =
        root().ok_or_else(|| io::Error::other("no folder of the user's own to unpack into"))?;
    own_root(&root)?;
    let prefix = format!("{kind}-{}-", std::process::id());
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix);
    // The folder is the user's alone, as its root (the umask would leave it 0755).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let dir = builder.tempdir_in(&root)?;
    Ok(dir.keep())
}

/// Removes this process's unpacked copies; how many folders went. Blocking: run it off the UI
/// thread.
pub fn remove_own() -> usize {
    let own = std::process::id();
    root().map_or(0, |root| sweep(&root, |pid| pid == own))
}

/// Removes the unpacked copies of processes that are no longer running, and their edit folders
/// that hold no kept change; how many folders went. Blocking: run it off the UI thread.
pub fn remove_stale() -> usize {
    let own = std::process::id();
    let stale = |pid| pid != own && !alive(pid);
    root().map_or(0, |root| sweep(&root, stale) + sweep_edits(&root, stale))
}

/// Removes the edit folders under `root` whose owner `stale` picks and which have no session
/// file beside them; how many went.
fn sweep_edits(root: &Path, stale: impl Fn(u32) -> bool) -> usize {
    use crate::remote_edit::folder;

    folder::folders(root, stale)
        .into_iter()
        .filter(|dir| !folder::session_path(dir).exists())
        .filter(|dir| remove(dir).is_ok())
        .count()
}

/// Removes the folders under `root` whose owner `stale` picks; how many went. A folder that
/// cannot be removed (a program still holds its file) is left for the next sweep.
fn sweep(root: &Path, stale: impl Fn(u32) -> bool) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(owner)
                .is_some_and(&stale)
        })
        // The entry itself, not where a link would lead.
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| remove(&entry.path()).is_ok())
        .count()
}

/// Removes one folder with its read-only copy; `remove_dir_all` does not follow links.
fn remove(dir: &Path) -> io::Result<()> {
    #[cfg(windows)]
    windows::clear_read_only(dir);
    fs::remove_dir_all(dir)
}

/// The process id in a folder name `open-<pid>-<random>` or `list-<pid>-<random>`; `None` for
/// any other name.
fn owner(name: &str) -> Option<u32> {
    let rest = name
        .strip_prefix("open-")
        .or_else(|| name.strip_prefix("list-"))?;
    let (pid, random) = rest.split_once('-')?;
    let digits = !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit());
    let letters = !random.is_empty() && random.bytes().all(|b| b.is_ascii_alphanumeric());
    if !digits || !letters {
        return None;
    }
    pid.parse().ok()
}

/// Whether the process `pid` is running.
#[cfg(windows)]
pub(crate) fn alive(pid: u32) -> bool {
    windows::alive(pid)
}

/// Whether the process `pid` is running: signal 0 to it, which macOS answers as Linux does (no
/// `/proc` there). Only "no such process" says it ended; a process of another user counts as
/// running. A pid no process can have (0, beyond `i32`) is not one.
#[cfg(not(windows))]
pub(crate) fn alive(pid: u32) -> bool {
    use nix::{errno::Errno, sys::signal::kill, unistd::Pid};

    match i32::try_from(pid) {
        Ok(pid) if pid > 0 => !matches!(kill(Pid::from_raw(pid), None), Err(Errno::ESRCH)),
        _ => false,
    }
}

#[cfg(test)]
#[path = "unpacked_tests.rs"]
mod tests;
