//! `file.set_comment` and the comments the properties dialog shows: a folder's `descript.ion`
//! read, changed and written back through its backend (`comments.rs` holds the format).
//!
//! Reading, changing and writing happen under one lock per folder (RC-110-24), so two comments
//! set at once - the window's and an MCP call's - are both kept. On this computer and on a server
//! the new file is written beside the old one and takes its place by a rename, as the compare
//! window saves (`compare/save.rs`): a full disk or a lost connection leaves the old comments.
//! A server whose rename does not replace has the old file removed first
//! (`rc_ops::replace_by_rename`, RC-110-25); if the rename fails after that, the complete new file
//! stays under its temporary name and the error names it. A device, an archive and the tests'
//! memory are written in place, as before.

use std::{
    collections::HashMap,
    io,
    path::Path,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

use rc_vfs::{BackendId, Entry, EntryKind, PropertyChange, Vfs, VfsError, VfsPath, WriteMode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::panel::resolve_path;
use crate::{
    Engine, EngineError, Outcome, PanelId,
    comments::{DESCRIPTION_FILE, Descriptions, MAX_COMMENT, MAX_FILE},
    recover::LockOrRecover as _,
};

/// The lock of the folder `dir`, held around reading, changing and writing its `descript.ion`.
/// One per folder for the whole process, since the file is one for all of it; names compare
/// without case, which at worst makes two folders on a case-sensitive file system take turns.
fn folder_lock(dir: &VfsPath) -> Arc<tokio::sync::Mutex<()>> {
    static FOLDERS: OnceLock<Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let key = format!(
        "{:?}\n{}",
        dir.backend,
        dir.path.to_string_lossy().to_lowercase()
    );
    let mut folders = FOLDERS
        .get_or_init(Mutex::default)
        .lock_or_recover("comment_folders");
    // Locks nobody holds any more go, so the map stays as small as the work in flight.
    folders.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = folders.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    folders.insert(key, Arc::downgrade(&lock));
    lock
}

/// Whether a new `descript.ion` may take the place of `now` by a rename: not of a link, which
/// would stop being one, and not of a read-only file, which stays refused as it was.
fn replaceable(now: &Entry) -> bool {
    !matches!(now.kind, EntryKind::Symlink { .. }) && !now.attributes.readonly
}

/// Writes the `descript.ion` at `file`. On this computer and on a server into a temporary file
/// beside it that takes its name once it is whole (on a disk: and synced), with the old file's
/// permission bits and its hidden and system attributes: a failure before that leaves the old
/// file as it was. The temporary name is no name the comments are read from, so what a run that
/// died left is never taken for them. Anywhere else, and over a link or a read-only file, in
/// place.
async fn store(vfs: &dyn Vfs, file: &VfsPath, bytes: &[u8]) -> Result<(), VfsError> {
    let now = match vfs.stat(&file.path).await {
        Ok(entry) => Some(entry),
        Err(VfsError::NotFound { .. }) => None,
        Err(error) => return Err(error),
    };
    let local = file.backend == BackendId::Local;
    let staged = local || file.backend.is_server();
    if !staged || now.as_ref().is_some_and(|now| !replaceable(now)) {
        return write(vfs, &file.path, bytes, WriteMode::Truncate).await;
    }
    let temp = rc_ops::temp_name::save_path(&file.path, nanos());
    let placed = match fill(vfs, &temp, bytes, now.as_ref()).await {
        // The local rename replaces in one step.
        Ok(()) if local => vfs.rename(&temp, &file.path).await,
        Ok(()) => match rc_ops::replace_by_rename(vfs, &temp, &file.path).await {
            Ok(()) => Ok(()),
            // The only copy of the folder's comments now: it stays, and the error names it.
            Err(unreplaced) if unreplaced.target_gone => {
                return Err(left_as(&temp, &unreplaced.error));
            }
            Err(unreplaced) => Err(unreplaced.error),
        },
        Err(error) => Err(error),
    };
    if placed.is_err() {
        let _ = vfs.remove_file(&temp).await;
    }
    placed
}

/// Writes `bytes` into the new file `temp` and gives it what the old file `now` had: its
/// permission bits and its hidden and system attributes.
async fn fill(
    vfs: &dyn Vfs,
    temp: &Path,
    bytes: &[u8],
    now: Option<&Entry>,
) -> Result<(), VfsError> {
    write(vfs, temp, bytes, WriteMode::CreateNew).await?;
    if let Some(now) = now {
        let flags = now.attributes.windows.unwrap_or_default();
        let keep = PropertyChange {
            mode: now.attributes.mode.filter(|_| vfs.capabilities().unix_mode),
            hidden: flags.hidden.then_some(true),
            system: flags.system.then_some(true),
            ..PropertyChange::default()
        };
        if keep != PropertyChange::default() {
            // As the compare window's save: a file that cannot take them is still written.
            let _ = vfs.set_properties(temp, keep).await;
        }
    }
    vfs.sync(temp).await
}

/// The error of a server that removed the old `descript.ion` and then refused the rename
/// (`cause`): the complete new file is still at `temp`, which the error names as its path.
fn left_as(temp: &Path, cause: &VfsError) -> VfsError {
    tracing::warn!(
        temp = %temp.display(),
        code = cause.code(),
        "the old descript.ion is gone and the new one could not be renamed; the complete new \
         file stays under its temporary name"
    );
    VfsError::Io {
        path: temp.to_path_buf(),
        source: io::Error::other(format!(
            "the old descript.ion was removed and the new one could not take its name \
             ({cause}); the folder's comments are complete in this file"
        )),
    }
}

async fn write(vfs: &dyn Vfs, path: &Path, bytes: &[u8], mode: WriteMode) -> Result<(), VfsError> {
    let mut stream = vfs.open_write(path, mode).await?;
    let io = |error| VfsError::from_io(error, path);
    stream.write_all(bytes).await.map_err(io)?;
    stream.shutdown().await.map_err(io)
}

/// The nanoseconds of the clock, which make a temporary name unique enough beside the process.
fn nanos() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos())
}

/// The comments of the folder `dir`; none when it has no `descript.ion`.
pub(crate) async fn read_descriptions(
    vfs: &Arc<dyn Vfs>,
    dir: &VfsPath,
) -> Result<Descriptions, EngineError> {
    let file = dir.join(DESCRIPTION_FILE);
    let entry = match vfs.stat(&file.path).await {
        Ok(entry) => entry,
        Err(VfsError::NotFound { .. }) => return Ok(Descriptions::default()),
        Err(error) => return Err(error.into()),
    };
    if entry.size > MAX_FILE {
        return Err(EngineError::PropertyInvalid {
            reason: "the folder's descript.ion is too large to read",
        });
    }
    let mut bytes = Vec::new();
    vfs.open_read(&file.path)
        .await?
        .take(MAX_FILE)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| VfsError::from_io(error, &file.path))?;
    Ok(Descriptions::from_bytes(&bytes))
}

/// Sets the comment of `name` in the `descript.ion` of `dir` - an empty one removes it, and a
/// file left empty goes too - and answers the folder's comments as they are then. Reading,
/// changing and writing happen under the folder's lock.
async fn change(
    vfs: &Arc<dyn Vfs>,
    dir: &VfsPath,
    name: &str,
    comment: &str,
) -> Result<Descriptions, EngineError> {
    let file = dir.join(DESCRIPTION_FILE);
    let lock = folder_lock(dir);
    let _held = lock.lock().await;
    let mut comments = read_descriptions(vfs, dir).await?;
    comments
        .set(name, Some(comment))
        .map_err(|error| EngineError::CommentNotEncodable {
            encoding: error.encoding,
        })?;
    if comments.is_empty() {
        match vfs.remove_file(&file.path).await {
            Ok(()) | Err(VfsError::NotFound { .. }) => {}
            Err(error) => return Err(error.into()),
        }
    } else {
        store(vfs.as_ref(), &file, &comments.to_bytes()).await?;
    }
    Ok(comments)
}

impl Engine {
    /// The comment of the entry at `path`, if its folder keeps one; unreadable is none.
    pub(crate) async fn comment_of(&self, vfs: &Arc<dyn Vfs>, path: &VfsPath) -> Option<String> {
        let dir = path.parent()?;
        let name = path.path.file_name()?.to_string_lossy();
        read_descriptions(vfs, &dir).await.ok()?.get(&name)
    }

    /// `file.set_comment`: the comment of `path` (or of the entry under the cursor); an empty
    /// one removes it, and a `descript.ion` left empty goes too. Only the comment's own line
    /// changes; one the file's encoding cannot hold is refused and nothing is written.
    pub(crate) async fn set_comment(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
        comment: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let Some(comment) = comment else {
            return Err(EngineError::PropertyInvalid {
                reason: "a comment to set, or an empty one to remove",
            });
        };
        if comment.chars().count() > MAX_COMMENT {
            return Err(EngineError::PropertyInvalid {
                reason: "a comment has 4096 characters at most",
            });
        }
        let id = self.resolve(panel);
        let target = {
            let state = self.state();
            let panel = state.panel(id);
            match path {
                Some(path) => resolve_path(&panel.path, path),
                None => panel
                    .cursor_entry()
                    .map(|entry| panel.path.join(&entry.name))
                    .ok_or(EngineError::PropertyInvalid {
                        reason: "a comment needs the entry it is about",
                    })?,
            }
        };
        let (Some(dir), Some(name)) = (target.parent(), target.path.file_name()) else {
            return Err(EngineError::PropertyInvalid {
                reason: "a comment needs the entry it is about",
            });
        };
        let name = name.to_string_lossy().into_owned();
        if name.contains(['"', '\n', '\r']) || name.eq_ignore_ascii_case(DESCRIPTION_FILE) {
            return Err(EngineError::PropertyInvalid {
                reason: "this name cannot carry a comment in descript.ion",
            });
        }
        let vfs = self.vfs(&target)?;
        vfs.stat(&target.path).await?;
        let comments = change(&vfs, &dir, &name, comment).await?;
        let file = dir.join(DESCRIPTION_FILE);
        self.refresh_showing(std::slice::from_ref(&file)).await;
        let comment = comments.get(&name);
        Ok(Outcome::CommentSet {
            path: target,
            comment,
        })
    }
}

#[cfg(test)]
#[path = "comment_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "comment_server_tests.rs"]
mod server_tests;
