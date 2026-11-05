//! Attributes, hidden entries, removal, details and links on Unix.

use std::{
    collections::HashMap,
    fs::{self, Metadata, Permissions},
    io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    sync::OnceLock,
};

use nix::unistd::{Gid, Group, Uid, User};

use crate::{
    Attributes, Details, FileId, LinkKind, LinkKinds, Owner, PropertyCapabilities, PropertyChange,
    Result, SetAttributes, VfsError, long_path,
};

/// Symbolic and hard links; no Windows flags, no creation time.
pub(super) const CAPABILITIES: PropertyCapabilities = PropertyCapabilities {
    links: LinkKinds {
        shortcut: false,
        symlink: true,
        hardlink: true,
        junction: false,
    },
    windows_flags: false,
    set_created: false,
};

/// Dot files are hidden.
pub(super) fn hidden(name: &str, _meta: &Metadata, _in_wsl: bool) -> bool {
    name.starts_with('.')
}

pub(super) fn attributes(meta: &Metadata) -> Attributes {
    Attributes::from_mode(meta.permissions().mode() & 0o7777)
}

/// The owners' names of one listing, each user and group looked up once: a folder of 100 000
/// files has a handful of owners.
#[derive(Default)]
pub(super) struct Owners {
    users: HashMap<u32, String>,
    groups: HashMap<u32, String>,
}

impl Owners {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// The owner of the entry `meta` describes; the number where the system knows no name.
    pub(super) fn of(&mut self, meta: &Metadata) -> Option<Owner> {
        let user = self
            .users
            .entry(meta.uid())
            .or_insert_with_key(|uid| user_name(*uid))
            .clone();
        let group = self
            .groups
            .entry(meta.gid())
            .or_insert_with_key(|gid| group_name(*gid))
            .clone();
        Some(Owner { user, group })
    }
}

fn user_name(uid: u32) -> String {
    match User::from_uid(Uid::from_raw(uid)) {
        Ok(Some(user)) => user.name,
        _ => uid.to_string(),
    }
}

fn group_name(gid: u32) -> String {
    match Group::from_gid(Gid::from_raw(gid)) {
        Ok(Some(group)) => group.name,
        _ => gid.to_string(),
    }
}

/// A file or a link; a link to a directory is unlinked like any other.
pub(super) fn remove_file(real: &Path) -> io::Result<()> {
    fs::remove_file(real)
}

/// A new mode, then read-only on top: read-only clears every write bit, writable sets the
/// owner's (never everyone's).
pub(super) fn set_attributes(path: &Path, change: SetAttributes) -> Result<()> {
    if change == SetAttributes::default() {
        return Ok(());
    }
    let real = long_path(path);
    let current = fs::metadata(&real).map_err(|error| VfsError::from_io(error, path))?;
    let mut mode = change.mode.unwrap_or_else(|| current.permissions().mode()) & 0o7777;
    match change.readonly {
        Some(true) => mode &= !0o222,
        Some(false) => mode |= 0o200,
        None => {}
    }
    fs::set_permissions(&real, Permissions::from_mode(mode))
        .map_err(|error| VfsError::from_io(error, path))
}

/// What `stat` tells beyond an entry: blocks, links, inode, owner. `_real` is the path, which
/// Windows needs and Unix does not.
pub(super) fn details(meta: &Metadata, _real: &Path) -> Details {
    let links = meta.nlink();
    Details {
        accessed: meta.accessed().ok(),
        allocated: Some(meta.blocks().saturating_mul(512)),
        links: Some(links),
        id: (links > 1 && !meta.is_dir()).then(|| FileId {
            device: meta.dev(),
            index: meta.ino(),
        }),
        uid: Some(meta.uid()),
        gid: Some(meta.gid()),
    }
}

/// Mode and read-only through `set_attributes`, the modification time through `filetime`;
/// the Windows flags and the creation time are refused before anything changes.
pub(super) fn set_properties(path: &Path, change: PropertyChange) -> Result<()> {
    if change.touches_windows_flags() || change.created.is_some() {
        return Err(VfsError::Unsupported {
            path: path.to_path_buf(),
            operation: if change.created.is_some() {
                "setting the creation time"
            } else {
                "setting the hidden, system or archive attribute"
            },
        });
    }
    set_attributes(
        path,
        SetAttributes {
            mode: change.mode,
            readonly: change.readonly,
        },
    )?;
    if let Some(modified) = change.modified {
        let real = long_path(path);
        let modified = filetime::FileTime::from_system_time(modified);
        let meta = fs::symlink_metadata(&real).map_err(|error| VfsError::from_io(error, path))?;
        let set = if meta.file_type().is_symlink() {
            // The link's own time: following it would change a file outside the selection.
            let accessed = filetime::FileTime::from_last_access_time(&meta);
            filetime::set_symlink_file_times(&real, accessed, modified)
        } else {
            filetime::set_file_mtime(&real, modified)
        };
        set.map_err(|error| VfsError::from_io(error, path))?;
    }
    Ok(())
}

/// A symbolic link stores `target` as given; a shortcut and a junction are Windows'.
pub(super) fn create_link(
    link: &Path,
    real: &Path,
    target: &Path,
    kind: LinkKind,
    _target_is_dir: bool,
) -> Result<()> {
    match kind {
        LinkKind::Symlink => {
            std::os::unix::fs::symlink(target, real).map_err(|error| VfsError::from_io(error, link))
        }
        LinkKind::Shortcut | LinkKind::Junction | LinkKind::Hardlink => {
            Err(VfsError::Unsupported {
                path: link.to_path_buf(),
                operation: "this kind of link",
            })
        }
    }
}

/// A rename that refuses an existing `to` in the one call (`renameat2` with `RENAME_NOREPLACE`);
/// `None` where the file system has no such rename (drvfs, 9p, an older kernel), so the caller
/// looks first.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(super) fn rename_no_replace(from: &Path, to: &Path) -> Option<io::Result<()>> {
    use nix::{
        errno::Errno,
        fcntl::{AT_FDCWD, RenameFlags, renameat2},
    };
    match renameat2(AT_FDCWD, from, AT_FDCWD, to, RenameFlags::RENAME_NOREPLACE) {
        Ok(()) => Some(Ok(())),
        Err(Errno::EINVAL | Errno::ENOSYS | Errno::EOPNOTSUPP) => None,
        Err(errno) => Some(Err(io::Error::from(errno))),
    }
}

/// Elsewhere (macOS, musl) the caller looks first.
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub(super) fn rename_no_replace(_from: &Path, _to: &Path) -> Option<io::Result<()>> {
    None
}

/// The process's file mode creation mask, read once (RC-080-70): from `/proc/self/status` where
/// Linux has it, else by setting a mask and putting the old one back - `umask(2)` cannot only
/// read. The mask set meanwhile is `077`, so a file another thread creates in between gets fewer
/// rights, never more.
pub(super) fn umask() -> u32 {
    static UMASK: OnceLock<u32> = OnceLock::new();
    *UMASK.get_or_init(|| {
        fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| parse_umask(&status))
            .unwrap_or_else(swap_umask)
    })
}

/// The `Umask:` line of a `/proc/<pid>/status`, octal.
pub(super) fn parse_umask(status: &str) -> Option<u32> {
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("Umask:"))?;
    u32::from_str_radix(value.trim(), 8)
        .ok()
        .map(|mask| mask & 0o777)
}

fn swap_umask() -> u32 {
    use nix::sys::stat::{Mode, umask};
    let old = umask(Mode::S_IRWXG | Mode::S_IRWXO);
    umask(old);
    #[allow(
        clippy::unnecessary_cast,
        reason = "`mode_t` is `u32` on Linux and `u16` on macOS"
    )]
    let bits = old.bits() as u32;
    bits & 0o777
}
