//! What a listing returns and what a backend can do.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

/// Seconds since 1970 as a time; `None` where the clock cannot hold them. A server's or an
/// archive's number never panics the caller, as `UNIX_EPOCH + duration` would on overflow.
#[must_use]
pub fn unix_time(seconds: u64) -> Option<SystemTime> {
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
}

/// `time` as whole seconds since 1970; `None` before 1970.
#[must_use]
pub fn unix_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs())
}

/// One directory entry, or the answer to a `stat`.
///
/// A symbolic link is described as a link (its kind carries the target), but its `size`,
/// `modified` and `created` are the target's when the target exists - what a file manager
/// shows for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The file name, without the directory. Converted lossily on Unix where a name is not
    /// UTF-8; every other backend has Unicode names.
    pub name: String,
    /// File, directory, link or something else.
    pub kind: EntryKind,
    /// Bytes of a file (or of a link's target file); 0 for directories and other kinds.
    pub size: u64,
    /// Last modification, when the backend reports it.
    pub modified: Option<SystemTime>,
    /// Creation, when the backend reports it (not every Unix file system does).
    pub created: Option<SystemTime>,
    /// Permissions and attribute flags.
    pub attributes: Attributes,
    /// Whether a panel hides the entry by default: a dot file on Unix and inside a WSL
    /// distribution, the hidden attribute on Windows.
    pub hidden: bool,
    /// Who owns the entry, where the backend tells (local Unix, SFTP); `None` elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
}

/// The owner of an [`Entry`]: user and group by name, or by number where the backend knows no
/// name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub user: String,
    pub group: String,
}

impl Entry {
    /// Whether the entry is a directory or a link to one: what a panel can enter.
    #[must_use]
    pub fn is_dir_like(&self) -> bool {
        matches!(
            self.kind,
            EntryKind::Dir
                | EntryKind::Symlink {
                    target_kind: Some(TargetKind::Dir),
                    ..
                }
        )
    }

    /// The modification time as whole seconds since 1970, as MCP answers and a conflict shows
    /// it; `None` when the backend reports none or it lies before 1970.
    #[must_use]
    pub fn modified_unix(&self) -> Option<u64> {
        self.modified.and_then(unix_seconds)
    }
}

/// The kind of an [`Entry`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link (on Windows also a junction).
    Symlink {
        /// The target as stored in the link, relative or absolute.
        target: PathBuf,
        /// What the target is; `None` for a dangling link.
        target_kind: Option<TargetKind>,
    },
    /// A socket, FIFO, device or anything else that is neither file nor directory.
    Other,
}

/// What a symbolic link points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    File,
    Dir,
    Other,
}

/// Permissions and attribute flags, in the terms of the platform that reported them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attributes {
    /// Unix permission bits (`st_mode & 0o7777`); `None` where the backend has none.
    pub mode: Option<u32>,
    /// Not writable: no write bit for anyone on Unix, the read-only attribute on Windows.
    pub readonly: bool,
    /// The Windows attribute flags beyond read-only; `None` off Windows.
    pub windows: Option<WindowsFlags>,
}

impl Attributes {
    /// The attributes of the Unix permission bits `mode` (`st_mode & 0o7777`, as the caller
    /// masked them): read-only when no write bit is set for anyone, no Windows flags. The one
    /// place the read-only rule of a mode is written.
    #[must_use]
    pub fn from_mode(mode: u32) -> Self {
        Self {
            mode: Some(mode),
            readonly: mode & 0o222 == 0,
            windows: None,
        }
    }
}

/// The Windows attribute flags a file manager shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsFlags {
    pub hidden: bool,
    pub system: bool,
    pub archive: bool,
}

/// A change of attributes: every `None` field stays as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetAttributes {
    /// New Unix permission bits; unsupported on Windows.
    pub mode: Option<u32>,
    /// Read-only on or off: the owner's write bit on Unix, the attribute on Windows.
    pub readonly: Option<bool>,
}

/// What a backend can do, so callers choose a strategy instead of trying and failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// `rename` can move an entry into another directory of the same backend.
    pub rename_across_dirs: bool,
    /// Listings can contain symbolic links.
    pub symlinks: bool,
    /// Names that differ only in case are different entries.
    pub case_sensitive: bool,
    /// Entries carry Unix permission bits and `set_attributes` accepts a mode.
    pub unix_mode: bool,
    /// `set_mtime` works.
    pub set_mtime: bool,
    /// `free_space` returns a number.
    pub free_space: bool,
}

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;
