//! What the properties dialog reads and changes beyond a listing (RC-020-06): links, the Windows
//! attribute flags, creation times, and the facts a size count needs (bytes on disk, hard links).
//!
//! Every backend gets these through defaulted [`crate::Vfs`] methods, so a backend that knows
//! none of it (an archive, FTP) needs no code: [`PropertyCapabilities::NONE`] tells the dialog to
//! grey the functions out instead of trying them.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// A kind of link `create_link` makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// A Windows shell shortcut (`.lnk`), Total Commander's Ctrl+Shift+F5.
    Shortcut,
    /// A symbolic link to a file or a directory.
    Symlink,
    /// A second name of a file on the same volume; never of a directory.
    Hardlink,
    /// A Windows junction: a link to a local directory that needs no privilege.
    Junction,
}

impl LinkKind {
    /// Every kind, in the order the link dialog offers them.
    pub const ALL: [Self; 4] = [
        Self::Shortcut,
        Self::Symlink,
        Self::Hardlink,
        Self::Junction,
    ];
}

/// The link kinds a backend can make.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkKinds {
    pub shortcut: bool,
    pub symlink: bool,
    pub hardlink: bool,
    pub junction: bool,
}

impl LinkKinds {
    /// Whether `kind` can be made.
    #[must_use]
    pub fn supports(self, kind: LinkKind) -> bool {
        match kind {
            LinkKind::Shortcut => self.shortcut,
            LinkKind::Symlink => self.symlink,
            LinkKind::Hardlink => self.hardlink,
            LinkKind::Junction => self.junction,
        }
    }

    /// Whether any kind can be made.
    #[must_use]
    pub fn any(self) -> bool {
        LinkKind::ALL.into_iter().any(|kind| self.supports(kind))
    }
}

/// What the properties dialog can change on a backend, beyond [`crate::Capabilities`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyCapabilities {
    /// The links `create_link` makes.
    pub links: LinkKinds,
    /// Hidden, system and archive can be set (Windows).
    pub windows_flags: bool,
    /// The creation time can be set (Windows).
    pub set_created: bool,
}

impl PropertyCapabilities {
    /// Nothing: the default of every backend.
    pub const NONE: Self = Self {
        links: LinkKinds {
            shortcut: false,
            symlink: false,
            hardlink: false,
            junction: false,
        },
        windows_flags: false,
        set_created: false,
    };
}

/// A change of attributes and times in one go: every `None` stays as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyChange {
    /// New Unix permission bits.
    #[serde(default)]
    pub mode: Option<u32>,
    /// Read-only on or off.
    #[serde(default)]
    pub readonly: Option<bool>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub system: Option<bool>,
    #[serde(default)]
    pub archive: Option<bool>,
    /// The modification time.
    #[serde(default)]
    pub modified: Option<SystemTime>,
    /// The creation time, where the file system keeps one that can be set.
    #[serde(default)]
    pub created: Option<SystemTime>,
}

impl PropertyChange {
    /// Whether a Windows attribute flag beyond read-only changes.
    #[must_use]
    pub fn touches_windows_flags(&self) -> bool {
        self.hidden.is_some() || self.system.is_some() || self.archive.is_some()
    }

    /// Whether nothing changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One file as the volume knows it: two names with the same id are one file (hard links).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileId {
    /// The device or the volume's serial number.
    pub device: u64,
    /// The inode or the file index.
    pub index: u64,
}

/// What [`crate::Vfs::details`] knows of an entry beyond its [`crate::Entry`]; every field is
/// `None` where the backend cannot tell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Details {
    /// The last access.
    pub accessed: Option<SystemTime>,
    /// Bytes the entry occupies on the volume (blocks, not length).
    pub allocated: Option<u64>,
    /// How many names the file has.
    pub links: Option<u64>,
    /// The file's identity, given only for a file with more than one name.
    pub id: Option<FileId>,
    /// The owner's user id (Unix).
    pub uid: Option<u32>,
    /// The owner's group id (Unix).
    pub gid: Option<u32>,
}
