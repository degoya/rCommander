//! Attributes, hidden entries, removal, details and links on Windows.
//!
//! The attribute flags, the file identity and the shortcut go through `winsafe`, whose safe
//! wrappers keep `unsafe` out of rCommander; the creation time through the standard library.
//! The junction is the one Win32 call without a safe wrapper (`junction.rs`).

mod junction;
mod shortcut;

use std::{
    fs::{self, FileTimes, Metadata, OpenOptions},
    io,
    os::windows::fs::{FileTimesExt, FileTypeExt, MetadataExt, OpenOptionsExt},
    path::Path,
};

use winsafe::{self as w, co};

use crate::{
    Attributes, Details, FileId, LinkKind, LinkKinds, Owner, PropertyCapabilities, PropertyChange,
    Result, SetAttributes, VfsError, WindowsFlags, long_path,
};

/// Every link kind, the flags and the creation time.
pub(super) const CAPABILITIES: PropertyCapabilities = PropertyCapabilities {
    links: LinkKinds {
        shortcut: true,
        symlink: true,
        hardlink: true,
        junction: true,
    },
    windows_flags: true,
    set_created: true,
};

/// `FILE_WRITE_ATTRIBUTES`: enough to set times, also on a read-only file.
const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
/// `FILE_FLAG_BACKUP_SEMANTICS`: lets a directory be opened.
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// `ERROR_PRIVILEGE_NOT_HELD`: a symbolic link without Developer Mode or administrator rights.
const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
const FILE_ATTRIBUTE_ARCHIVE: u32 = 0x20;

/// No owner column on Windows: the owner is a security descriptor, not a name a listing reads.
#[derive(Default)]
pub(super) struct Owners;

impl Owners {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) fn of(&mut self, _meta: &Metadata) -> Option<Owner> {
        None
    }
}

/// The hidden attribute; inside a WSL distribution also dot files, which Linux hides and the
/// share does not mark.
pub(super) fn hidden(name: &str, meta: &Metadata, in_wsl: bool) -> bool {
    meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 || (in_wsl && name.starts_with('.'))
}

pub(super) fn attributes(meta: &Metadata) -> Attributes {
    let flags = meta.file_attributes();
    Attributes {
        mode: None,
        readonly: flags & FILE_ATTRIBUTE_READONLY != 0,
        windows: Some(WindowsFlags {
            hidden: flags & FILE_ATTRIBUTE_HIDDEN != 0,
            system: flags & FILE_ATTRIBUTE_SYSTEM != 0,
            archive: flags & FILE_ATTRIBUTE_ARCHIVE != 0,
        }),
    }
}

/// A file or a link; a directory link (or junction) is a directory to Windows and goes with
/// `remove_dir`, which removes the link and never the target's contents.
pub(super) fn remove_file(real: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(real)?.file_type();
    if kind.is_symlink_dir() {
        fs::remove_dir(real)
    } else {
        fs::remove_file(real)
    }
}

/// Read-only only: Windows has no Unix mode, and the other attribute flags need a Win32 call
/// that the standard library does not wrap (and `unsafe` is denied).
pub(super) fn set_attributes(path: &Path, change: SetAttributes) -> Result<()> {
    if change.mode.is_some() {
        return Err(VfsError::Unsupported {
            path: path.to_path_buf(),
            operation: "setting a Unix mode",
        });
    }
    let Some(readonly) = change.readonly else {
        return Ok(());
    };
    let real = long_path(path);
    let mut permissions = fs::metadata(&real)
        .map_err(|error| VfsError::from_io(error, path))?
        .permissions();
    permissions.set_readonly(readonly);
    fs::set_permissions(&real, permissions).map_err(|error| VfsError::from_io(error, path))
}

/// The access time from the metadata; the link count and identity of a file from its handle
/// (`GetFileInformationByHandle`), which a listing does not report.
pub(super) fn details(meta: &Metadata, real: &Path) -> Details {
    let (links, id) = if meta.is_file() {
        identity(real).map_or((None, None), |(links, id)| {
            (Some(links), (links > 1).then_some(id))
        })
    } else {
        (None, None)
    };
    Details {
        accessed: meta.accessed().ok(),
        allocated: None,
        links,
        id,
        uid: None,
        gid: None,
    }
}

/// The number of names of the file at `real` and its identity; `None` when it cannot be opened.
fn identity(real: &Path) -> Option<(u64, FileId)> {
    let name = real.to_str()?;
    let (file, _) = w::HFILE::CreateFile(
        name,
        co::GENERIC::default(),
        Some(co::FILE_SHARE::READ | co::FILE_SHARE::WRITE | co::FILE_SHARE::DELETE),
        None,
        co::DISPOSITION::OPEN_EXISTING,
        co::FILE_ATTRIBUTE::NORMAL,
        Some(co::FILE_FLAG::BACKUP_SEMANTICS | co::FILE_FLAG::OPEN_REPARSE_POINT),
        None,
        None,
    )
    .ok()?;
    let info = file.GetFileInformationByHandle().ok()?;
    Some((
        u64::from(info.nNumberOfLinks),
        FileId {
            device: u64::from(info.dwVolumeSerialNumber),
            index: info.nFileIndex(),
        },
    ))
}

/// The attribute flags through `SetFileAttributesW`, the times through a handle; a Unix mode is
/// refused before anything changes.
pub(super) fn set_properties(path: &Path, change: PropertyChange) -> Result<()> {
    if change.mode.is_some() {
        return Err(VfsError::Unsupported {
            path: path.to_path_buf(),
            operation: "setting a Unix mode",
        });
    }
    let real = long_path(path);
    if change.readonly.is_some() || change.touches_windows_flags() {
        let name = real.to_str().ok_or_else(|| VfsError::InvalidPath {
            path: path.to_path_buf(),
            reason: "not valid Unicode",
        })?;
        let current = w::GetFileAttributes(name).map_err(|error| win_error(error, path))?;
        let settable = co::FILE_ATTRIBUTE::READONLY
            | co::FILE_ATTRIBUTE::HIDDEN
            | co::FILE_ATTRIBUTE::SYSTEM
            | co::FILE_ATTRIBUTE::ARCHIVE
            | co::FILE_ATTRIBUTE::NOT_CONTENT_INDEXED
            | co::FILE_ATTRIBUTE::OFFLINE
            | co::FILE_ATTRIBUTE::TEMPORARY;
        let mut flags = current & settable;
        for (flag, wanted) in [
            (co::FILE_ATTRIBUTE::READONLY, change.readonly),
            (co::FILE_ATTRIBUTE::HIDDEN, change.hidden),
            (co::FILE_ATTRIBUTE::SYSTEM, change.system),
            (co::FILE_ATTRIBUTE::ARCHIVE, change.archive),
        ] {
            match wanted {
                Some(true) => flags |= flag,
                Some(false) => flags &= !flag,
                None => {}
            }
        }
        if flags == co::FILE_ATTRIBUTE::default() {
            flags = co::FILE_ATTRIBUTE::NORMAL;
        }
        w::SetFileAttributes(name, flags).map_err(|error| win_error(error, path))?;
    }
    if change.modified.is_some() || change.created.is_some() {
        let file = OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(&real)
            .map_err(|error| VfsError::from_io(error, path))?;
        let mut times = FileTimes::new();
        if let Some(modified) = change.modified {
            times = times.set_modified(modified);
        }
        if let Some(created) = change.created {
            times = times.set_created(created);
        }
        file.set_times(times)
            .map_err(|error| VfsError::from_io(error, path))?;
    }
    Ok(())
}

/// A rename that refuses an existing `to` in the one call: `MoveFileExW` without
/// `MOVEFILE_REPLACE_EXISTING` (and without `MOVEFILE_COPY_ALLOWED`, so another volume stays
/// "not the same device", as for [`std::fs::rename`]). winsafe takes `&str`: a path that is no
/// UTF-8 gets `None`, and the caller looks first.
pub(super) fn rename_no_replace(from: &Path, to: &Path) -> Option<io::Result<()>> {
    let (from, to) = (from.to_str()?, to.to_str()?);
    Some(
        w::MoveFileEx(from, Some(to), co::MOVEFILE::default())
            .map_err(|error| io::Error::from_raw_os_error(u32::from(error) as i32)),
    )
}

/// A Win32 error code as the error of `path`.
fn win_error(error: co::ERROR, path: &Path) -> VfsError {
    VfsError::from_io(io::Error::from_raw_os_error(u32::from(error) as i32), path)
}

/// A symbolic link of the kind its target is, a junction or a shortcut; the hard link is
/// portable (`links.rs`).
pub(super) fn create_link(
    link: &Path,
    real: &Path,
    target: &Path,
    kind: LinkKind,
    target_is_dir: bool,
) -> Result<()> {
    let made = match kind {
        LinkKind::Symlink if target_is_dir => std::os::windows::fs::symlink_dir(target, real),
        LinkKind::Symlink => std::os::windows::fs::symlink_file(target, real),
        LinkKind::Junction => junction::create(real, target),
        LinkKind::Shortcut => return shortcut::create(link, real, target),
        LinkKind::Hardlink => fs::hard_link(long_path(target), real),
    };
    made.map_err(|error| {
        if kind == LinkKind::Symlink && error.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) {
            VfsError::SymlinkPrivilege {
                path: link.to_path_buf(),
            }
        } else {
            VfsError::from_io(error, link)
        }
    })
}
