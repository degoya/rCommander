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
//! junction, and nothing else under the root.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(windows)]
mod windows;

/// The folder all unpacked copies live under; `None` without a folder of the user's own.
#[cfg(windows)]
fn root() -> Option<PathBuf> {
    Some(std::env::temp_dir().join("rcommander"))
}

/// The folder all unpacked copies live under; `None` without a folder of the user's own.
#[cfg(not(windows))]
fn root() -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    let base = dirs.runtime_dir().unwrap_or_else(|| dirs.cache_dir());
    Some(base.join("rcommander").join("unpacked"))
}

/// Creates `root` when it is missing, as a folder of this user's alone.
#[cfg(windows)]
fn own_root(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)
}

/// Creates `root` when it is missing, as a folder of this user's alone: mode 0700, and a real
/// folder, not a link to one elsewhere.
#[cfg(not(windows))]
fn own_root(root: &Path) -> io::Result<()> {
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
    tokio::task::spawn_blocking(create_folder)
        .await
        .map_err(io::Error::other)?
}

/// [`new_folder`], blocking.
fn create_folder() -> io::Result<PathBuf> {
    let root =
        root().ok_or_else(|| io::Error::other("no folder of the user's own to unpack into"))?;
    own_root(&root)?;
    let prefix = format!("open-{}-", std::process::id());
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

/// Removes the unpacked copies of processes that are no longer running; how many folders went.
/// Blocking: run it off the UI thread.
pub fn remove_stale() -> usize {
    let own = std::process::id();
    root().map_or(0, |root| sweep(&root, |pid| pid != own && !alive(pid)))
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

/// The process id in a folder name `open-<pid>-<random>`; `None` for any other name.
fn owner(name: &str) -> Option<u32> {
    let (pid, random) = name.strip_prefix("open-")?.split_once('-')?;
    let digits = !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit());
    let letters = !random.is_empty() && random.bytes().all(|b| b.is_ascii_alphanumeric());
    if !digits || !letters {
        return None;
    }
    pid.parse().ok()
}

/// Whether the process `pid` is running.
#[cfg(windows)]
fn alive(pid: u32) -> bool {
    windows::alive(pid)
}

/// Whether the process `pid` is running, read from `/proc`. Without one (macOS) nothing tells,
/// and a folder is kept rather than taken from a running app.
#[cfg(not(windows))]
fn alive(pid: u32) -> bool {
    let proc = Path::new("/proc");
    !proc.is_dir() || proc.join(pid.to_string()).exists()
}

#[cfg(test)]
#[path = "unpacked_tests.rs"]
mod tests;
