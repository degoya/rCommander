//! Reading two files for the edit mode and writing a side back (RC-100-94), on any [`Vfs`].
//!
//! [`load`] reads both files whole as the line diff does and judges each side: inside an
//! archive, past [`EDIT_LIMIT`] or not written back to the same bytes it stays read-only.
//! [`save`] writes one side: first it looks whether the file is still as it was read (size and
//! time) and refuses with `compare_changed` when not, unless told to overwrite. On the local
//! backend (and WSL through it) the bytes go into a temporary file beside it that then takes the
//! file's place, so a failed write leaves the old file whole; a link is written through, as is a
//! file on a server (SFTP, FTP), which has no such rename.

use std::time::{SystemTime, UNIX_EPOCH};

use rc_vfs::{BackendId, EntryKind, SetAttributes, Vfs, VfsError, VfsPath, WriteMode};
use tokio::io::AsyncWriteExt as _;
use tokio_util::sync::CancellationToken;

use super::{
    Side,
    edit::{EDIT_LIMIT, EditSession, Origin, ReadOnly, SideFile},
    read_bounded,
    text::{self, TextFile},
};
use crate::EngineError;

/// Both files as an [`EditSession`]; `None` when either is binary, larger than
/// [`super::TEXT_DIFF_LIMIT`] or of too many lines, as the line diff answers.
///
/// # Errors
///
/// Either path is a directory or cannot be read, or `cancel` was cancelled.
pub async fn load(
    left: Side<'_>,
    right: Side<'_>,
    cancel: &CancellationToken,
) -> Result<Option<EditSession>, EngineError> {
    let Some(left) = load_side(left, cancel).await? else {
        return Ok(None);
    };
    let Some(right) = load_side(right, cancel).await? else {
        return Ok(None);
    };
    Ok(Some(EditSession::new(left, right)))
}

async fn load_side(
    side: Side<'_>,
    cancel: &CancellationToken,
) -> Result<Option<SideFile>, EngineError> {
    let entry = side.vfs.stat(&side.path.path).await?;
    if entry.is_dir_like() {
        return Err(VfsError::IsADirectory {
            path: side.path.path.clone(),
        }
        .into());
    }
    let Some(bytes) = read_bounded(side, cancel).await? else {
        return Ok(None);
    };
    let Some(file) = text::decode(&bytes) else {
        return Ok(None);
    };
    let read_only = read_only(side.path, &bytes, &file);
    // The size read, not the size listed: a file that grew between the two is judged by what
    // was read, and a save then finds it changed.
    let origin = Origin {
        path: side.path.clone(),
        size: bytes.len() as u64,
        modified: entry.modified,
    };
    Ok(Some(SideFile {
        file,
        origin,
        read_only,
    }))
}

/// Why a side of these bytes, read as `file`, is not edited.
fn read_only(path: &VfsPath, bytes: &[u8], file: &TextFile) -> Option<ReadOnly> {
    if matches!(path.backend, BackendId::Archive { .. }) {
        Some(ReadOnly::Archive)
    } else if bytes.len() as u64 > EDIT_LIMIT {
        Some(ReadOnly::TooLarge)
    } else if text::encode(file).ok().as_deref() != Some(bytes) {
        Some(ReadOnly::NotLossless)
    } else {
        None
    }
}

/// Writes `bytes` as the file `origin` names, when it is still as `origin` says or `force`;
/// answers how it is then.
///
/// # Errors
///
/// `compare_changed` when the file changed and not `force`, `archive_read_only` inside an
/// archive, or what the file system answers.
pub async fn save(
    vfs: &dyn Vfs,
    origin: &Origin,
    bytes: &[u8],
    force: bool,
) -> Result<Origin, EngineError> {
    let path = &origin.path;
    if matches!(path.backend, BackendId::Archive { .. }) {
        return Err(EngineError::ArchiveReadOnly);
    }
    let now = vfs.stat(&path.path).await?;
    if !force && !origin.unchanged(&now) {
        return Err(EngineError::CompareChanged {
            name: crate::shown_path(path).display().to_string(),
        });
    }
    let link = matches!(now.kind, EntryKind::Symlink { .. });
    if path.backend == BackendId::Local && !link {
        let temp = rc_ops::temp_name::save_path(&path.path, nanos());
        if let Err(error) = write(vfs, &temp, bytes, WriteMode::CreateNew).await {
            let _ = vfs.remove_file(&temp).await;
            return Err(error.into());
        }
        // The permission bits go with it: a script stays executable.
        if let Some(mode) = now.attributes.mode
            && vfs.capabilities().unix_mode
        {
            let keep = SetAttributes {
                mode: Some(mode),
                readonly: None,
            };
            let _ = vfs.set_attributes(&temp, keep).await;
        }
        let placed = match vfs.sync(&temp).await {
            Ok(()) => vfs.rename(&temp, &path.path).await,
            Err(error) => Err(error),
        };
        if let Err(error) = placed {
            let _ = vfs.remove_file(&temp).await;
            return Err(error.into());
        }
    } else {
        write(vfs, &path.path, bytes, WriteMode::Truncate).await?;
    }
    let written = vfs.stat(&path.path).await?;
    Ok(Origin::of(path.clone(), &written))
}

async fn write(
    vfs: &dyn Vfs,
    path: &std::path::Path,
    bytes: &[u8],
    mode: WriteMode,
) -> Result<(), VfsError> {
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

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
