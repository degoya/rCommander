//! An in-memory backend: a whole file system in one map, for the tests of everything that works
//! against [`Vfs`] - the operations queue first of all, which must never know whether its
//! source and target are local.
//!
//! Paths are absolute (`/dir/file`), names are case-sensitive, there are no links. Two knobs
//! simulate what a test cannot provoke on a real disk: a capacity
//! ([`MemoryFs::with_capacity`]), past which a write fails with `StorageFull` - a full disk - and
//! the read-only flag (`set_attributes`), which on a directory refuses creating and removing in
//! it - no access. A file being written is visible with what it holds so far, as on a disk.
//! Compiled for this crate's tests and behind the `memory` feature.

mod tree;

use std::{
    io,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    task::{Context, Poll},
    time::SystemTime,
};

use async_trait::async_trait;
use tokio::io::AsyncWrite;

use crate::{
    Capabilities, Entry, ReadStream, Result, SetAttributes, Vfs, VfsError, WriteMode, WriteStream,
};
use tree::Tree;

/// A file system in memory. Clones share the same files.
#[derive(Debug, Clone)]
pub struct MemoryFs {
    tree: Arc<Mutex<Tree>>,
}

impl Default for MemoryFs {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryFs {
    /// An empty file system: only the root directory `/`, unlimited space.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tree: Arc::new(Mutex::new(Tree::new(None))),
        }
    }

    /// An empty file system that holds at most `bytes` bytes of file contents.
    #[must_use]
    pub fn with_capacity(bytes: u64) -> Self {
        Self {
            tree: Arc::new(Mutex::new(Tree::new(Some(bytes)))),
        }
    }

    fn tree(&self) -> MutexGuard<'_, Tree> {
        lock(&self.tree)
    }
}

fn lock(tree: &Mutex<Tree>) -> MutexGuard<'_, Tree> {
    tree.lock().unwrap_or_else(PoisonError::into_inner)
}

#[async_trait]
impl Vfs for MemoryFs {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            rename_across_dirs: true,
            symlinks: false,
            case_sensitive: true,
            unix_mode: false,
            set_mtime: true,
            free_space: self.tree().capacity.is_some(),
        }
    }

    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        self.tree().list(dir)
    }

    async fn stat(&self, path: &Path) -> Result<Entry> {
        self.tree().entry(path)
    }

    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        let tree = self.tree();
        let node = tree.node(path)?;
        let data = node.data.clone().ok_or_else(|| VfsError::IsADirectory {
            path: path.to_path_buf(),
        })?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        let tree = self.tree();
        let node = tree.node(path)?;
        let data = node.data.as_ref().ok_or_else(|| VfsError::IsADirectory {
            path: path.to_path_buf(),
        })?;
        let from = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(data.len());
        Ok(Box::new(io::Cursor::new(data[from..].to_vec())))
    }

    async fn open_write(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        let key = match mode {
            WriteMode::CreateNew => self.tree().open_write(path, false, true)?,
            WriteMode::Truncate => self.tree().open_write(path, true, false)?,
            WriteMode::Append => self.tree().open_write(path, false, false)?,
        };
        Ok(Box::new(MemoryWriter {
            tree: Arc::clone(&self.tree),
            key,
        }))
    }

    async fn mkdir(&self, path: &Path, recursive: bool) -> Result<()> {
        self.tree().mkdir(path, recursive)
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.tree().rename(from, to)
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        self.tree().remove_file(path)
    }

    async fn remove_dir(&self, path: &Path, recursive: bool) -> Result<()> {
        self.tree().remove_dir(path, recursive)
    }

    async fn set_mtime(&self, path: &Path, modified: SystemTime) -> Result<()> {
        self.tree().node_mut(path)?.modified = modified;
        Ok(())
    }

    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> Result<()> {
        if change.mode.is_some() {
            return Err(VfsError::Unsupported {
                path: path.to_path_buf(),
                operation: "setting a Unix mode",
            });
        }
        let mut tree = self.tree();
        let node = tree.node_mut(path)?;
        if let Some(readonly) = change.readonly {
            node.readonly = readonly;
        }
        Ok(())
    }

    async fn free_space(&self, path: &Path) -> Result<Option<u64>> {
        let tree = self.tree();
        tree.node(path)?;
        Ok(tree.free())
    }
}

/// A file being written: every write lands in the map at once.
struct MemoryWriter {
    tree: Arc<Mutex<Tree>>,
    key: std::path::PathBuf,
}

impl AsyncWrite for MemoryWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(lock(&self.tree).append(&self.key, bytes))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
