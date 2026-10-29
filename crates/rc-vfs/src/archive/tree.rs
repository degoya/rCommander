//! The directory tree of an archive, built once from the flat entry list every format stores and
//! kept until the archive file changes.
//!
//! Directories the archive names only through paths below them (`a/b/c.txt` without `a/`) are
//! made up. Where two entries land on one path the later one wins, as when unpacking - except
//! that an entry with a safe name is never replaced by one whose name had to be cleaned, and a
//! directory with entries is never replaced by a file.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use super::safety::{UnsafeName, clean_name};
use crate::{Attributes, Entry, EntryKind, Result, VfsError};

/// What an archive says about one entry beyond the [`Entry`] every backend returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveEntryInfo {
    /// The name as the archive stores it, decoded; empty for a directory that was made up.
    pub stored_name: String,
    /// Why the stored name was not safe as a path; the entry is listed under its cleaned path.
    pub unsafe_name: Option<UnsafeName>,
    /// The entry is encrypted: reading it needs a password.
    pub encrypted: bool,
    /// Bytes the entry takes inside the archive, where the format tells per entry.
    pub packed_size: Option<u64>,
    /// A directory the archive has no entry for, made up from the paths below it.
    pub implied: bool,
}

/// An entry with what the archive says about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveItem {
    pub entry: Entry,
    pub info: ArchiveEntryInfo,
}

/// The kind of a stored entry, as a format reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RawKind {
    File,
    Dir,
    /// A symbolic link with its target as stored (empty where the format hides it).
    Symlink(String),
    /// A hard link, a device or anything else that is not read.
    Other,
}

/// One entry as a format scan reports it, before its name is cleaned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawEntry {
    pub name: String,
    pub kind: RawKind,
    pub size: u64,
    pub packed: Option<u64>,
    pub modified: Option<SystemTime>,
    pub mode: Option<u32>,
    pub readonly: bool,
    pub hidden: bool,
    pub encrypted: bool,
    /// Where the format finds the entry again: its index (zip, 7z) or its position (tar).
    pub index: u64,
}

impl RawEntry {
    /// An entry of `kind` with every other field empty.
    pub(crate) fn new(name: impl Into<String>, kind: RawKind, index: u64) -> Self {
        Self {
            name: name.into(),
            kind,
            size: 0,
            packed: None,
            modified: None,
            mode: None,
            readonly: false,
            hidden: false,
            encrypted: false,
            index,
        }
    }
}

/// One node of the tree.
#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub entry: Entry,
    pub info: ArchiveEntryInfo,
    /// The format's index of the entry; `None` for a made-up directory.
    pub index: Option<u64>,
    children: BTreeMap<String, usize>,
}

impl Node {
    fn implied_dir(name: String) -> Self {
        Self {
            entry: Entry {
                name,
                kind: EntryKind::Dir,
                size: 0,
                modified: None,
                created: None,
                attributes: Attributes::default(),
                hidden: false,
                owner: None,
            },
            info: ArchiveEntryInfo {
                stored_name: String::new(),
                unsafe_name: None,
                encrypted: false,
                packed_size: None,
                implied: true,
            },
            index: None,
            children: BTreeMap::new(),
        }
    }

    fn from_raw(name: String, raw: RawEntry, unsafe_name: Option<UnsafeName>) -> Self {
        let (kind, size) = match raw.kind {
            RawKind::File => (EntryKind::File, raw.size),
            RawKind::Dir => (EntryKind::Dir, 0),
            // Never resolved: inside an archive a link is shown, not followed.
            RawKind::Symlink(target) => (
                EntryKind::Symlink {
                    target: PathBuf::from(target),
                    target_kind: None,
                },
                0,
            ),
            RawKind::Other => (EntryKind::Other, 0),
        };
        let hidden = raw.hidden || name.starts_with('.');
        Self {
            entry: Entry {
                name,
                kind,
                size,
                modified: raw.modified,
                created: None,
                attributes: Attributes {
                    mode: raw.mode,
                    readonly: raw.readonly,
                    windows: None,
                },
                hidden,
                owner: None,
            },
            info: ArchiveEntryInfo {
                stored_name: raw.name,
                unsafe_name,
                encrypted: raw.encrypted,
                packed_size: raw.packed,
                implied: false,
            },
            index: Some(raw.index),
            children: BTreeMap::new(),
        }
    }

    pub(crate) fn is_dir(&self) -> bool {
        self.entry.kind == EntryKind::Dir
    }

    pub(crate) fn item(&self) -> ArchiveItem {
        ArchiveItem {
            entry: self.entry.clone(),
            info: self.info.clone(),
        }
    }
}

/// The tree; node 0 is the root.
#[derive(Debug, Clone)]
pub(crate) struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    /// Builds the tree from a scan, in the order the archive stores the entries.
    pub(crate) fn build(raws: Vec<RawEntry>) -> Self {
        let mut tree = Self {
            nodes: vec![Node::implied_dir(String::new())],
        };
        for raw in raws {
            let clean = clean_name(&raw.name);
            let Some((leaf, parents)) = clean.parts.split_last() else {
                // `./`, `/` or `..`: nothing to show.
                continue;
            };
            let Some(parent) = tree.parent_dir(parents) else {
                // A file stands where a directory of this path would be.
                continue;
            };
            let node = Node::from_raw(leaf.clone(), raw, clean.issue);
            tree.place(parent, node);
        }
        tree
    }

    /// The directory `parts` names, made up where missing; `None` when a file is in the way.
    fn parent_dir(&mut self, parts: &[String]) -> Option<usize> {
        let mut current = 0;
        for part in parts {
            current = match self.nodes[current].children.get(part).copied() {
                Some(child) if self.nodes[child].is_dir() => child,
                Some(_) => return None,
                None => self.add(current, Node::implied_dir(part.clone())),
            };
        }
        Some(current)
    }

    fn add(&mut self, parent: usize, node: Node) -> usize {
        let index = self.nodes.len();
        let name = node.entry.name.clone();
        self.nodes.push(node);
        self.nodes[parent].children.insert(name, index);
        index
    }

    fn place(&mut self, parent: usize, mut node: Node) {
        let Some(existing) = self.nodes[parent].children.get(&node.entry.name).copied() else {
            self.add(parent, node);
            return;
        };
        let old = &self.nodes[existing];
        let old_safe = old.info.unsafe_name.is_none() && !old.info.implied;
        let new_unsafe = node.info.unsafe_name.is_some();
        if old.is_dir() && node.is_dir() {
            // A directory named twice, or named after its contents: keep the contents, take the
            // stated metadata unless only an unsafe name states it.
            if !(old_safe && new_unsafe) {
                node.children = std::mem::take(&mut self.nodes[existing].children);
                self.nodes[existing] = node;
            }
            return;
        }
        let keeps_contents = old.is_dir() && !old.children.is_empty();
        if keeps_contents || (old_safe && new_unsafe) {
            return;
        }
        // The node replaced stays in the vector, unreachable.
        self.nodes[existing] = node;
    }

    /// The node at `path` (`/` or empty is the root).
    pub(crate) fn find(&self, path: &Path) -> Result<&Node> {
        let mut current = &self.nodes[0];
        for component in path.components() {
            let part = match component {
                Component::RootDir | Component::CurDir => continue,
                Component::Normal(part) => part.to_str().ok_or_else(|| VfsError::InvalidPath {
                    path: path.to_path_buf(),
                    reason: "not UTF-8",
                })?,
                Component::ParentDir | Component::Prefix(_) => {
                    return Err(VfsError::InvalidPath {
                        path: path.to_path_buf(),
                        reason: "not a path inside the archive",
                    });
                }
            };
            if !current.is_dir() {
                return Err(VfsError::NotADirectory {
                    path: path.to_path_buf(),
                });
            }
            let child = current
                .children
                .get(part)
                .ok_or_else(|| VfsError::NotFound {
                    path: path.to_path_buf(),
                })?;
            current = &self.nodes[*child];
        }
        Ok(current)
    }

    /// The entries of a directory node, by name.
    pub(crate) fn children<'a>(&'a self, node: &'a Node) -> impl Iterator<Item = &'a Node> + 'a {
        node.children.values().map(|&index| &self.nodes[index])
    }
}

/// A Windows FILETIME (100 ns since 1601) as a time; `None` for 0, which means unset.
pub(crate) fn file_time(ticks: u64) -> Option<SystemTime> {
    const TICKS_PER_SECOND: u64 = 10_000_000;
    const SECONDS_1601_TO_1970: u64 = 11_644_473_600;
    if ticks == 0 {
        return None;
    }
    let seconds = ticks / TICKS_PER_SECOND;
    // Below 10^7, times 100: below 10^9, so it fits.
    let nanos = u32::try_from((ticks % TICKS_PER_SECOND) * 100).ok()?;
    match seconds.checked_sub(SECONDS_1601_TO_1970) {
        Some(after) => SystemTime::UNIX_EPOCH.checked_add(Duration::new(after, nanos)),
        None => SystemTime::UNIX_EPOCH
            .checked_sub(Duration::from_secs(SECONDS_1601_TO_1970 - seconds))
            .and_then(|time| time.checked_add(Duration::from_nanos(u64::from(nanos)))),
    }
}

/// The permission bits of a stored mode: the set-uid, set-gid and sticky bits an archive's author
/// chose never reach a copy out of the archive, as they never reach an unpacked file
/// (RC-080-12).
pub(crate) fn permission_bits(mode: u32) -> u32 {
    mode & 0o777
}

/// Read-only by the stored mode: no write bit for anyone ([`Attributes::from_mode`]).
pub(crate) fn readonly_mode(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| Attributes::from_mode(mode).readonly)
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
