//! Where the copies live: one folder per edited file, `<root>/edit-<pid>-<random>`, under the
//! root of the unpacked copies (`unpacked.rs`: the user's own, mode 0700), named by `tempfile`
//! so no other process guesses it. Beside each folder, `edit-<pid>-<random>.json` holds what
//! the copy is held against - the server file, its baseline, the stamp of the copy when it was
//! last in step - and no content and no secret. With it the folder outlives a quit that keeps
//! a change, and the next start offers it (`kept.rs`); `unpacked::remove_stale` spares a folder
//! with one.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{Baseline, Stamp};
use crate::VfsPath;

/// What starts an edit folder's name.
pub(crate) const PREFIX: &str = "edit-";

/// The version of the session file this build writes and reads.
const SESSION_VERSION: u32 = 1;

/// The largest session file read; one is a few hundred bytes.
const MAX_SESSION_BYTES: u64 = 64 * 1024;

/// What a session file holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Session {
    pub version: u32,
    /// The server file as it was opened.
    pub shown: VfsPath,
    /// The file an upload replaces: `shown`, or where its link leads.
    pub remote: VfsPath,
    /// The connection's name.
    pub server: String,
    /// The copy's name inside the folder.
    pub name: String,
    pub baseline: Baseline,
    /// The copy as it was when it was last in step with the server.
    pub synced: Option<Stamp>,
    /// Uploads write into the server's file itself (links, another owner).
    #[serde(default)]
    pub in_place: bool,
}

impl Session {
    /// A session of this build's version.
    pub(crate) fn new(
        (shown, remote): (VfsPath, VfsPath),
        server: String,
        name: String,
        baseline: Baseline,
        synced: Option<Stamp>,
        in_place: bool,
    ) -> Self {
        Self {
            version: SESSION_VERSION,
            shown,
            remote,
            server,
            name,
            baseline,
            synced,
            in_place,
        }
    }
}

/// Creates the folder for one edited file under `root` (made the user's own first). Blocking.
///
/// # Errors
///
/// The root or the folder cannot be created.
pub(crate) fn create(root: &Path) -> io::Result<PathBuf> {
    crate::unpacked::own_root(root)?;
    let prefix = format!("{PREFIX}{}-", std::process::id());
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    Ok(builder.tempdir_in(root)?.keep())
}

/// The session file of the folder `dir`: `<dir>.json` beside it.
pub(crate) fn session_path(dir: &Path) -> PathBuf {
    let mut name = dir.as_os_str().to_owned();
    name.push(".json");
    PathBuf::from(name)
}

/// Writes the session of `dir`, replacing the one there through a temporary file. Blocking.
///
/// # Errors
///
/// The file cannot be written.
pub(crate) fn write_session(dir: &Path, session: &Session) -> io::Result<()> {
    let path = session_path(dir);
    let text = serde_json::to_vec_pretty(session).map_err(io::Error::other)?;
    let mut temporary = path.clone().into_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, text)?;
    fs::rename(&temporary, &path)
}

/// The session of `dir`; `None` when there is none, or one of another version or shape.
/// Blocking.
pub(crate) fn read_session(dir: &Path) -> Option<Session> {
    let path = session_path(dir);
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SESSION_BYTES {
        return None;
    }
    let session: Session = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
    (session.version == SESSION_VERSION).then_some(session)
}

/// Removes the folder `dir` with its copy and its session file. Blocking; a folder a program
/// still holds stays for a later sweep.
pub(crate) fn remove(dir: &Path) {
    let _ = fs::remove_file(session_path(dir));
    #[cfg(windows)]
    clear_read_only(dir);
    let _ = fs::remove_dir_all(dir);
}

/// Clears a read-only attribute an editor left on the copy, so its folder goes.
#[cfg(windows)]
#[allow(
    clippy::permissions_set_readonly_false,
    reason = "Windows: this clears the read-only attribute, there is no Unix mode to widen"
)]
fn clear_read_only(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if let Ok(metadata) = entry.metadata()
            && metadata.is_file()
        {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                let _ = fs::set_permissions(entry.path(), permissions);
            }
        }
    }
}

/// The process id in a folder name `edit-<pid>-<random>`; `None` for any other name.
pub(crate) fn owner(name: &str) -> Option<u32> {
    let (pid, random) = name.strip_prefix(PREFIX)?.split_once('-')?;
    let digits = !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit());
    let letters = !random.is_empty() && random.bytes().all(|b| b.is_ascii_alphanumeric());
    if !digits || !letters {
        return None;
    }
    pid.parse().ok()
}

/// The edit folders under `root` whose owner `pick` takes: real folders only, never a link.
/// Blocking.
pub(crate) fn folders(root: &Path, pick: impl Fn(u32) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(owner)
                .is_some_and(&pick)
        })
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect()
}

#[cfg(test)]
#[path = "folder_tests.rs"]
mod tests;
