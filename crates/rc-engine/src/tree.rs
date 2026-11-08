//! The folder trees (RC-100-88): Total Commander's tree in a panel (Ctrl+F8), the separate tree
//! beside the panels (Ctrl+Shift+F8) and the tree dialog (Alt+F10) all show the folders of one
//! location, read lazily - a folder lists its sub-folders when it is unfolded, never a whole
//! drive, so no `treeinfo.wc` is written anywhere and no network or server drive is walked
//! (owner 2026-11-07; the deviation from Total Commander is reasoned in the job file).
//!
//! The engine keeps what was read, one level per folder ([`store`]), bounded to [`MAX_NODES`]
//! folders: past it the subtrees the window folded go first. A level is read again after a
//! panel showing it was read again (`panel.refresh`, the refresh after a file operation) or when
//! the window asks (F2). Links to folders are listed as folders but never unfolded, so no link
//! makes a cycle. When a panel follows the tree's cursor is [`Follow`]'s to say; the window's
//! timer only reports the pause.

mod follow;
mod store;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::{Entry, EntryKind, Vfs, VfsPath};

pub use follow::{FOLLOW_PAUSE, Follow};
pub(crate) use store::TreeStore;
pub use store::{MAX_LEVEL, MAX_NODES};

use crate::{
    EngineError, SortOrder,
    sort::{names_for, sort_entries},
    state::is_hidden,
};

/// One sub-folder of a level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeChild {
    pub name: String,
    /// Hidden or system: shown only while the panels show hidden entries.
    pub hidden: bool,
    /// A link (or junction) to a folder: listed, never unfolded.
    pub link: bool,
}

/// The sub-folders of one folder, in the panels' name order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeLevel {
    pub path: VfsPath,
    pub children: Arc<[TreeChild]>,
    /// More than [`MAX_LEVEL`] sub-folders: only the first are listed.
    pub truncated: bool,
}

impl TreeLevel {
    /// The level as a tree shows it: without the hidden folders unless `show_hidden`.
    #[must_use]
    pub fn shown(&self, show_hidden: bool) -> Self {
        if show_hidden || !self.children.iter().any(|child| child.hidden) {
            return self.clone();
        }
        Self {
            path: self.path.clone(),
            children: self
                .children
                .iter()
                .filter(|child| !child.hidden)
                .cloned()
                .collect(),
            truncated: self.truncated,
        }
    }
}

/// Whether the tree beside the panels is off, one for both panels or one beside each
/// (`panels.separate_tree`, Total Commander's `SeparateTree=0/1/2`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SeparateTree {
    #[default]
    Off,
    /// One tree left of both panels, of the active panel.
    One,
    /// A tree left of each panel.
    Two,
}

impl SeparateTree {
    /// The stored value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::One => "one",
            Self::Two => "two",
        }
    }

    /// The stored value read; anything else is off.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value {
            "one" => Self::One,
            "two" => Self::Two,
            _ => Self::Off,
        }
    }

    /// Ctrl+Shift+F8's next one, as Total Commander turns them: off, one, two, off.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::One,
            Self::One => Self::Two,
            Self::Two => Self::Off,
        }
    }
}

/// The root of the location `path` is on, as Total Commander's tree starts at the drive: `C:\`,
/// `\\server\share\`, `/`, the root of an archive or of a server.
#[must_use]
pub fn tree_root(path: &VfsPath) -> VfsPath {
    let root = path
        .path
        .ancestors()
        .last()
        .map_or_else(PathBuf::new, Path::to_path_buf);
    VfsPath {
        backend: path.backend.clone(),
        path: root,
    }
}

/// The folders from the root of `path`'s location down to `path`, the root first: the levels a
/// tree reads to show `path` unfolded.
#[must_use]
pub fn tree_chain(path: &VfsPath) -> Vec<VfsPath> {
    let mut chain: Vec<VfsPath> = path
        .path
        .ancestors()
        .map(|folder| VfsPath {
            backend: path.backend.clone(),
            path: folder.to_path_buf(),
        })
        .collect();
    chain.reverse();
    chain
}

/// The sub-folders of `path` on `vfs`, sorted as the panels sort names, at most [`MAX_LEVEL`].
pub(crate) async fn read_level(vfs: &dyn Vfs, path: &VfsPath) -> Result<TreeLevel, EngineError> {
    let mut folders: Vec<Entry> = vfs
        .list(&path.path)
        .await?
        .into_iter()
        .filter(Entry::is_dir_like)
        .collect();
    sort_entries(&mut folders, SortOrder::default(), names_for(path));
    let truncated = folders.len() > MAX_LEVEL;
    folders.truncate(MAX_LEVEL);
    let children = folders
        .into_iter()
        .map(|entry| TreeChild {
            hidden: is_hidden(&entry),
            link: matches!(entry.kind, EntryKind::Symlink { .. }),
            name: entry.name,
        })
        .collect();
    Ok(TreeLevel {
        path: path.clone(),
        children,
        truncated,
    })
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
