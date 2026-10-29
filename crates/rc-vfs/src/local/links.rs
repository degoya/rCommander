//! Creating links on the local file system, the part every platform shares: never over an
//! existing entry, hard links only to files on the same volume, junctions only to directories,
//! shortcuts always named `.lnk`. The platform module makes the link itself.

use std::{fs, io, path::Path};

use super::platform;
use crate::{LinkKind, Result, VfsError, long_path};

/// The extension of a Windows shortcut.
pub(super) const SHORTCUT_EXTENSION: &str = "lnk";

/// Makes a link of `kind` at `link` leading to `target`, both as the caller spells them.
pub(super) fn create_link(link: &Path, target: &Path, kind: LinkKind) -> Result<()> {
    if kind == LinkKind::Shortcut
        && !link
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(SHORTCUT_EXTENSION))
    {
        return Err(VfsError::InvalidPath {
            path: link.to_path_buf(),
            reason: "a shortcut's name ends in .lnk",
        });
    }
    let real = long_path(link);
    if fs::symlink_metadata(&real).is_ok() {
        return Err(VfsError::AlreadyExists {
            path: link.to_path_buf(),
        });
    }
    // A relative target is relative to the link's folder, as the file system reads it.
    let resolved = match link.parent() {
        Some(dir) if target.is_relative() => dir.join(target),
        _ => target.to_path_buf(),
    };
    let followed = fs::metadata(long_path(&resolved));
    match kind {
        LinkKind::Hardlink => {
            let meta = followed.map_err(|error| VfsError::from_io(error, target))?;
            if meta.is_dir() {
                return Err(VfsError::IsADirectory {
                    path: target.to_path_buf(),
                });
            }
            fs::hard_link(long_path(&resolved), &real).map_err(|error| hard_link_error(error, link))
        }
        LinkKind::Junction => {
            let meta = followed.map_err(|error| VfsError::from_io(error, target))?;
            if !meta.is_dir() {
                return Err(VfsError::NotADirectory {
                    path: target.to_path_buf(),
                });
            }
            platform::create_link(link, &real, &resolved, kind, true)
        }
        LinkKind::Shortcut => {
            followed.map_err(|error| VfsError::from_io(error, target))?;
            platform::create_link(link, &real, &resolved, kind, false)
        }
        // A dangling symbolic link is allowed, as `ln -s` allows it.
        LinkKind::Symlink => {
            let is_dir = followed.is_ok_and(|meta| meta.is_dir());
            platform::create_link(link, &real, target, kind, is_dir)
        }
    }
}

/// A hard link across volumes says so, rather than a bare I/O error.
fn hard_link_error(error: io::Error, link: &Path) -> VfsError {
    if error.kind() == io::ErrorKind::CrossesDevices {
        return VfsError::InvalidPath {
            path: link.to_path_buf(),
            reason: "a hard link must be on the volume of its file",
        };
    }
    VfsError::from_io(error, link)
}
