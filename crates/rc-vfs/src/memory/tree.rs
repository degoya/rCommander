//! The state of a [`super::MemoryFs`]: every node by its path, and the bytes in use.

use std::{
    collections::BTreeMap,
    io,
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

use crate::{Attributes, Entry, EntryKind, Result, VfsError};

/// A file or a directory.
#[derive(Debug, Clone)]
pub(super) struct Node {
    pub(super) data: Option<Vec<u8>>,
    pub(super) modified: SystemTime,
    pub(super) readonly: bool,
}

impl Node {
    fn dir() -> Self {
        Self {
            data: None,
            modified: SystemTime::now(),
            readonly: false,
        }
    }

    fn file() -> Self {
        Self {
            data: Some(Vec::new()),
            modified: SystemTime::now(),
            readonly: false,
        }
    }

    pub(super) fn is_dir(&self) -> bool {
        self.data.is_none()
    }

    fn entry(&self, name: String) -> Entry {
        Entry {
            hidden: name.starts_with('.'),
            name,
            kind: if self.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            size: self.data.as_ref().map_or(0, |data| data.len() as u64),
            modified: Some(self.modified),
            created: None,
            attributes: Attributes {
                mode: None,
                readonly: self.readonly,
                windows: None,
            },
            owner: None,
        }
    }
}

/// Every node by its key: the path's normal components, the root being the empty path.
#[derive(Debug)]
pub(super) struct Tree {
    nodes: BTreeMap<PathBuf, Node>,
    used: u64,
    pub(super) capacity: Option<u64>,
}

/// The key of `path`: absolute paths only, `.` dropped, `..` refused.
pub(super) fn key(path: &Path) -> Result<PathBuf> {
    let invalid = |reason| VfsError::InvalidPath {
        path: path.to_path_buf(),
        reason,
    };
    if !path.has_root() {
        return Err(invalid("not absolute"));
    }
    let mut key = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => key.push(name),
            Component::ParentDir => return Err(invalid("contains ..")),
            Component::Prefix(_) => return Err(invalid("has a drive prefix")),
        }
    }
    Ok(key)
}

fn not_found(path: &Path) -> VfsError {
    VfsError::NotFound {
        path: path.to_path_buf(),
    }
}

impl Tree {
    pub(super) fn new(capacity: Option<u64>) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(PathBuf::new(), Node::dir());
        Self {
            nodes,
            used: 0,
            capacity,
        }
    }

    pub(super) fn node(&self, path: &Path) -> Result<&Node> {
        self.nodes.get(&key(path)?).ok_or_else(|| not_found(path))
    }

    /// The directory `path` must be in for something to be created there.
    fn writable_parent(&self, path: &Path, key: &Path) -> Result<()> {
        let parent = key.parent().ok_or_else(|| VfsError::AlreadyExists {
            path: path.to_path_buf(),
        })?;
        match self.nodes.get(parent) {
            None => Err(not_found(path)),
            Some(node) if !node.is_dir() => Err(VfsError::NotADirectory {
                path: path.to_path_buf(),
            }),
            Some(node) if node.readonly => Err(VfsError::PermissionDenied {
                path: path.to_path_buf(),
            }),
            Some(_) => Ok(()),
        }
    }

    pub(super) fn entry(&self, path: &Path) -> Result<Entry> {
        let key = key(path)?;
        let node = self.nodes.get(&key).ok_or_else(|| not_found(path))?;
        let name = key.file_name().map_or_else(
            || "/".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        Ok(node.entry(name))
    }

    /// The keys directly inside `key`.
    fn children(&self, key: &Path) -> Vec<PathBuf> {
        self.nodes
            .range(key.to_path_buf()..)
            .skip(1)
            .map(|(child, _)| child)
            .take_while(|child| child.starts_with(key))
            .filter(|child| child.parent() == Some(key))
            .cloned()
            .collect()
    }

    pub(super) fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        let key = key(dir)?;
        match self.nodes.get(&key) {
            None => return Err(not_found(dir)),
            Some(node) if !node.is_dir() => {
                return Err(VfsError::NotADirectory {
                    path: dir.to_path_buf(),
                });
            }
            Some(_) => {}
        }
        Ok(self
            .children(&key)
            .into_iter()
            .filter_map(|child| {
                let name = child.file_name()?.to_string_lossy().into_owned();
                Some(self.nodes.get(&child)?.entry(name))
            })
            .collect())
    }

    /// Prepares `path` for writing in `mode`; the key to write to.
    pub(super) fn open_write(
        &mut self,
        path: &Path,
        truncate: bool,
        fresh: bool,
    ) -> Result<PathBuf> {
        let key = key(path)?;
        if let Some(node) = self.nodes.get_mut(&key) {
            if node.is_dir() {
                return Err(VfsError::IsADirectory {
                    path: path.to_path_buf(),
                });
            }
            if fresh {
                return Err(VfsError::AlreadyExists {
                    path: path.to_path_buf(),
                });
            }
            if node.readonly {
                return Err(VfsError::PermissionDenied {
                    path: path.to_path_buf(),
                });
            }
            if truncate && let Some(data) = node.data.as_mut() {
                self.used -= data.len() as u64;
                data.clear();
                node.modified = SystemTime::now();
            }
            return Ok(key);
        }
        self.writable_parent(path, &key)?;
        self.nodes.insert(key.clone(), Node::file());
        Ok(key)
    }

    /// Appends to the file at `key` as far as the capacity allows; the bytes taken.
    pub(super) fn append(&mut self, key: &Path, bytes: &[u8]) -> io::Result<usize> {
        let room = self
            .capacity
            .map_or(u64::MAX, |capacity| capacity.saturating_sub(self.used));
        if room == 0 && !bytes.is_empty() {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        let taken = usize::try_from(room).map_or(bytes.len(), |room| room.min(bytes.len()));
        let node = self
            .nodes
            .get_mut(key)
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        let data = node
            .data
            .as_mut()
            .ok_or_else(|| io::Error::from(io::ErrorKind::IsADirectory))?;
        data.extend_from_slice(&bytes[..taken]);
        node.modified = SystemTime::now();
        self.used += taken as u64;
        Ok(taken)
    }

    pub(super) fn mkdir(&mut self, path: &Path, recursive: bool) -> Result<()> {
        let key = key(path)?;
        if let Some(node) = self.nodes.get(&key) {
            return if recursive && node.is_dir() {
                Ok(())
            } else {
                Err(VfsError::AlreadyExists {
                    path: path.to_path_buf(),
                })
            };
        }
        if recursive && let Some(parent) = key.parent() {
            let missing = !self.nodes.contains_key(parent);
            if missing {
                self.mkdir(&Path::new("/").join(parent), true)
                    .map_err(|_| not_found(path))?;
            }
        }
        self.writable_parent(path, &key)?;
        self.nodes.insert(key, Node::dir());
        Ok(())
    }

    pub(super) fn rename(&mut self, from: &Path, to: &Path) -> Result<()> {
        let source = key(from)?;
        let target = key(to)?;
        let moving = self.nodes.get(&source).ok_or_else(|| not_found(from))?;
        let is_dir = moving.is_dir();
        if source == target {
            return Ok(());
        }
        if target.starts_with(&source) {
            return Err(VfsError::InvalidPath {
                path: to.to_path_buf(),
                reason: "inside the source",
            });
        }
        match self.nodes.get(&target) {
            Some(existing) if existing.is_dir() || is_dir => {
                return Err(VfsError::AlreadyExists {
                    path: to.to_path_buf(),
                });
            }
            _ => {}
        }
        self.writable_parent(to, &target)?;
        if let Some(replaced) = self.nodes.remove(&target) {
            self.used -= replaced.data.map_or(0, |data| data.len() as u64);
        }
        let moved: Vec<PathBuf> = self
            .nodes
            .range(source.clone()..)
            .map(|(key, _)| key)
            .take_while(|key| key.starts_with(&source))
            .cloned()
            .collect();
        for old in moved {
            let rest = old.strip_prefix(&source).map(Path::to_path_buf);
            if let (Ok(rest), Some(node)) = (rest, self.nodes.remove(&old)) {
                let new = if rest.as_os_str().is_empty() {
                    target.clone()
                } else {
                    target.join(rest)
                };
                self.nodes.insert(new, node);
            }
        }
        Ok(())
    }

    pub(super) fn remove_file(&mut self, path: &Path) -> Result<()> {
        let key = key(path)?;
        match self.nodes.get(&key) {
            None => Err(not_found(path)),
            Some(node) if node.is_dir() => Err(VfsError::IsADirectory {
                path: path.to_path_buf(),
            }),
            Some(_) => {
                if let Some(parent) = key.parent()
                    && self.nodes.get(parent).is_some_and(|dir| dir.readonly)
                {
                    return Err(VfsError::PermissionDenied {
                        path: path.to_path_buf(),
                    });
                }
                if let Some(node) = self.nodes.remove(&key) {
                    self.used -= node.data.map_or(0, |data| data.len() as u64);
                }
                Ok(())
            }
        }
    }

    pub(super) fn remove_dir(&mut self, path: &Path, recursive: bool) -> Result<()> {
        let key = key(path)?;
        match self.nodes.get(&key) {
            None => return Err(not_found(path)),
            Some(node) if !node.is_dir() => {
                return Err(VfsError::NotADirectory {
                    path: path.to_path_buf(),
                });
            }
            Some(_) if key.as_os_str().is_empty() => {
                return Err(VfsError::InvalidPath {
                    path: path.to_path_buf(),
                    reason: "the root",
                });
            }
            Some(_) => {}
        }
        if !recursive && !self.children(&key).is_empty() {
            return Err(VfsError::DirectoryNotEmpty {
                path: path.to_path_buf(),
            });
        }
        let doomed: Vec<PathBuf> = self
            .nodes
            .range(key.clone()..)
            .map(|(child, _)| child)
            .take_while(|child| child.starts_with(&key))
            .cloned()
            .collect();
        for child in doomed {
            if let Some(node) = self.nodes.remove(&child) {
                self.used -= node.data.map_or(0, |data| data.len() as u64);
            }
        }
        Ok(())
    }

    pub(super) fn node_mut(&mut self, path: &Path) -> Result<&mut Node> {
        self.nodes
            .get_mut(&key(path)?)
            .ok_or_else(|| not_found(path))
    }

    /// Bytes left before writes fail with `StorageFull`.
    pub(super) fn free(&self) -> Option<u64> {
        self.capacity
            .map(|capacity| capacity.saturating_sub(self.used))
    }
}
