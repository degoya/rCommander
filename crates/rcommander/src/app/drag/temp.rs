//! The temporary folder that remote files are fetched into before they are dragged out or put
//! on the clipboard (RC-080-01): one folder per session in the system's temporary folder, made
//! only for this user (`0700` where the system has modes), one subfolder per fetch, all of it
//! removed when rCommander closes. A session that crashed leaves its folder behind; the next
//! session that fetches sweeps such folders once they are a day old.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// The start of a session folder's name in the system's temporary folder.
pub const PREFIX: &str = "rcommander-drag-";
/// A session folder this old belongs to a session that is long gone.
pub const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
/// How often a taken name is tried again with the next number.
const ATTEMPTS: u32 = 16;

/// The session's temporary folder, once made, and the fetches in it.
#[derive(Debug, Default)]
pub struct TempArea {
    session: Option<PathBuf>,
    fetches: u64,
}

impl TempArea {
    pub fn session(&self) -> Option<&Path> {
        self.session.as_deref()
    }

    /// The session folder [`make_session`] made.
    pub fn set_session(&mut self, folder: PathBuf) {
        self.session = Some(folder);
    }

    /// A new subfolder for one fetch, inside the session folder; the job that fetches makes it.
    pub fn next_folder(&mut self) -> Option<PathBuf> {
        let session = self.session.as_ref()?;
        self.fetches += 1;
        Some(session.join(self.fetches.to_string()))
    }

    /// The session folder, handed over for [`remove_off_thread`]; a later fetch makes a new one.
    pub fn take_session(&mut self) -> Option<PathBuf> {
        self.session.take()
    }

    /// Removes the session folder with everything fetched into it.
    pub fn clean_up(&mut self) -> io::Result<()> {
        match self.session.take() {
            Some(folder) => remove(&folder),
            None => Ok(()),
        }
    }
}

impl Drop for TempArea {
    fn drop(&mut self) {
        // A folder that cannot be removed now is swept by a later session.
        let _ = self.clean_up();
    }
}

/// Removes a session folder ([`TempArea::take_session`]) with its contents on the blocking pool,
/// not on the window's thread. A folder that cannot be removed now is swept by a later session.
pub async fn remove_off_thread(folder: PathBuf) {
    let _ = tokio::task::spawn_blocking(move || remove(&folder)).await;
}

/// [`make_session`] in the system's temporary folder, on the blocking pool.
pub async fn make_session_off_thread() -> Result<PathBuf, String> {
    let root = std::env::temp_dir();
    match tokio::task::spawn_blocking(move || make_session(&root)).await {
        Ok(made) => made.map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Makes a new session folder in `root` (the system's temporary folder), only for this user, and
/// sweeps the stale ones of earlier sessions. Blocking: run off the window's thread.
pub fn make_session(root: &Path) -> io::Result<PathBuf> {
    sweep(root, SystemTime::now());
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
    for attempt in 0..ATTEMPTS {
        let folder = root.join(format!("{PREFIX}{}-{stamp}-{attempt}", std::process::id()));
        // `create_dir`, not `create_dir_all`: a folder someone else made under the name is no
        // folder of ours.
        match private_dir().create(&folder) {
            Ok(()) => return Ok(folder),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

#[cfg(unix)]
fn private_dir() -> fs::DirBuilder {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    builder
}

/// On Windows the temporary folder is the user's own already.
#[cfg(not(unix))]
fn private_dir() -> fs::DirBuilder {
    fs::DirBuilder::new()
}

/// Removes the session folders in `root` older than [`STALE_AFTER`] at `now`; how many. Links
/// are never followed, and a folder that cannot be removed (another user's) is left.
pub fn sweep(root: &Path, now: SystemTime) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(PREFIX) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let stale = meta
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STALE_AFTER);
        if meta.is_dir() && stale && remove(&entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Removes `folder` with its contents; a folder that is gone already is fine.
fn remove(folder: &Path) -> io::Result<()> {
    match fs::remove_dir_all(folder) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
#[path = "temp_tests.rs"]
mod tests;
