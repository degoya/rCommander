//! What an icon is cached by: the extension for ordinary files, the file itself for the few
//! kinds that carry their own icon, one key for every plain folder.

use std::path::PathBuf;

use rc_engine::{Entry, VfsPath, extension};
use rc_vfs::BackendId;

/// The extensions whose files show their own icon: programs, shortcuts, icon files, Internet
/// shortcuts (as in Explorer).
const OWN_ICON: &[&str] = &["exe", "lnk", "ico", "url"];

/// The longest extension cached as one; a longer "extension" is part of a name, not a type.
const MAX_EXT: usize = 16;

/// The cache key of an icon, and the request to the provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IconKey {
    /// A plain folder, and the `..` row.
    Folder,
    /// A file by its lower-case extension; empty for none.
    Ext(Box<str>),
    /// A local file or folder with an icon of its own, by its full path.
    File(PathBuf),
}

impl IconKey {
    /// The key of `entry`, listed in `dir`.
    ///
    /// Only the local file system gets per-file keys: the inside of an archive and a server's
    /// files are shown by their extension, so no icon ever opens a file there. A folder with the
    /// read-only or system attribute may have a `desktop.ini` with an icon of its own, which is
    /// how Explorer tells them apart too.
    pub fn of(entry: &Entry, dir: &VfsPath) -> Self {
        let local = dir.backend == BackendId::Local;
        if entry.is_dir_like() {
            let custom = entry.attributes.readonly
                || entry.attributes.windows.is_some_and(|flags| flags.system);
            return if local && custom {
                Self::File(dir.path.join(&entry.name))
            } else {
                Self::Folder
            };
        }
        let ext = extension(entry);
        if ext.len() > MAX_EXT {
            return Self::Ext("".into());
        }
        let ext = ext.to_ascii_lowercase();
        if local && OWN_ICON.contains(&ext.as_str()) {
            Self::File(dir.path.join(&entry.name))
        } else {
            Self::Ext(ext.into())
        }
    }

    /// Whether the key names one file, which costs a file access to load.
    pub fn per_file(&self) -> bool {
        matches!(self, Self::File(_))
    }
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod tests;
