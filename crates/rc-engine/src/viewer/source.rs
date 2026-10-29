//! A file opened for viewing: read in windows anywhere in it, never as a whole.
//!
//! The text and hex views read through [`FileWindow::read_at`], which keeps the last
//! [`CACHE_BLOCKS`] blocks of [`BLOCK_SIZE`] bytes - 4 MiB whatever the file's size - so
//! scrolling back and forth reads nothing twice. Search and the line index read once from start
//! to end and go around the cache ([`FileWindow::stream_from`], [`FileWindow::read_uncached`]).
//! A backend that can seek ([`Vfs::open_read_seekable`]) starts each read where it is wanted, and
//! so does one that can start a read at an offset ([`Vfs::open_read_at`]: FTP's `REST`); one that
//! can only stream (an archive) skips to it.

use std::{
    collections::VecDeque,
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

use rc_vfs::{EntryKind, ReadStream, TargetKind, Vfs, VfsError};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// The unit the cache keeps.
pub const BLOCK_SIZE: usize = 64 * 1024;

/// How many blocks the cache keeps: what a view holds of a file, whatever its size.
pub const CACHE_BLOCKS: usize = 64;

/// One file, opened for viewing.
pub struct FileWindow {
    vfs: Arc<dyn Vfs>,
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
    /// Cleared the first time the backend answers that it cannot seek.
    seekable: AtomicBool,
    /// Cleared the first time the backend answers that it cannot start a read at an offset.
    resumable: AtomicBool,
    cache: Mutex<BlockCache>,
}

impl std::fmt::Debug for FileWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileWindow")
            .field("path", &self.path)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl FileWindow {
    /// Opens `path` on `vfs`: reads its size and nothing of its contents.
    ///
    /// # Errors
    ///
    /// The path does not exist, is a directory (`is_a_directory`) or a device, socket or pipe
    /// (`unsupported`), which a viewer could wait on forever.
    pub async fn open(vfs: Arc<dyn Vfs>, path: impl Into<PathBuf>) -> Result<Self, VfsError> {
        let path = path.into();
        let entry = vfs.stat(&path).await?;
        match entry.kind {
            EntryKind::Dir
            | EntryKind::Symlink {
                target_kind: Some(TargetKind::Dir),
                ..
            } => return Err(VfsError::IsADirectory { path }),
            EntryKind::Other
            | EntryKind::Symlink {
                target_kind: Some(TargetKind::Other),
                ..
            } => {
                return Err(VfsError::Unsupported {
                    path,
                    operation: "viewing a device, socket or pipe",
                });
            }
            EntryKind::File | EntryKind::Symlink { .. } => {}
        }
        Ok(Self {
            vfs,
            path,
            size: entry.size,
            modified: entry.modified,
            seekable: AtomicBool::new(true),
            resumable: AtomicBool::new(true),
            cache: Mutex::new(BlockCache::default()),
        })
    }

    /// The file's path on its backend.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The size when the file was opened; [`FileWindow::changed`] tells whether it moved on.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The modification time when the file was opened.
    #[must_use]
    pub fn modified(&self) -> Option<SystemTime> {
        self.modified
    }

    /// Whether the file's size or modification time differ from when it was opened - the
    /// moment to offer "reload" (open it again).
    ///
    /// # Errors
    ///
    /// The file cannot be read any more.
    pub async fn changed(&self) -> Result<bool, VfsError> {
        let entry = self.vfs.stat(&self.path).await?;
        Ok(entry.size != self.size || entry.modified != self.modified)
    }

    /// The same file opened again: its new size, an empty cache.
    ///
    /// # Errors
    ///
    /// As [`FileWindow::open`].
    pub async fn reload(&self) -> Result<Self, VfsError> {
        Self::open(Arc::clone(&self.vfs), self.path.clone()).await
    }

    /// Whether the backend is known to reach an offset only by reading the file up to it: it
    /// answered that it can neither seek nor start a read there. A search going backwards over
    /// such a file reads it once forwards instead of once per chunk (RC-080-46).
    #[must_use]
    pub fn skips_to_offsets(&self) -> bool {
        !self.seekable.load(Ordering::Relaxed) && !self.resumable.load(Ordering::Relaxed)
    }

    /// Whether the backend answered that it cannot seek and has not refused to start a read at
    /// an offset (FTP's `REST`): each such read is a new connection, so a search going backwards
    /// reads long spans on one stream each instead of one read per chunk (RC-080-57).
    #[must_use]
    pub fn resumes_at_offsets(&self) -> bool {
        !self.seekable.load(Ordering::Relaxed) && self.resumable.load(Ordering::Relaxed)
    }

    /// Bytes the cache holds now; never more than [`CACHE_BLOCKS`] times [`BLOCK_SIZE`].
    #[must_use]
    pub fn cached_bytes(&self) -> usize {
        self.cache().bytes()
    }

    /// Up to `len` bytes at `offset`, through the cache; fewer at the end of the file, none
    /// past it.
    ///
    /// # Errors
    ///
    /// The backend failed to read.
    pub async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, VfsError> {
        let end = self.size.min(offset.saturating_add(len as u64));
        if offset >= end {
            return Ok(Vec::new());
        }
        let block = BLOCK_SIZE as u64;
        let (first, last) = (offset / block, (end - 1) / block);
        let mut blocks = Vec::new();
        let mut missing = Vec::new();
        {
            let mut cache = self.cache();
            for index in first..=last {
                match cache.get(index) {
                    Some(data) => blocks.push((index, data)),
                    None => missing.push(index),
                }
            }
        }
        if let (Some(&from), Some(&to)) = (missing.first(), missing.last()) {
            // One read for the whole gap; blocks in it that were cached are read again.
            let span = usize::try_from((to - from + 1) * block).unwrap_or(usize::MAX);
            let bytes = self.read_uncached(from * block, span).await?;
            let mut cache = self.cache();
            for (index, chunk) in (from..).zip(bytes.chunks(BLOCK_SIZE)) {
                let data: Arc<[u8]> = Arc::from(chunk);
                cache.insert(index, Arc::clone(&data));
                if missing.contains(&index) {
                    blocks.push((index, data));
                }
            }
        }
        blocks.sort_by_key(|(index, _)| *index);
        let mut out = Vec::with_capacity(usize::try_from(end - offset).unwrap_or(0));
        for (index, data) in blocks {
            let start = index * block;
            let from = usize::try_from(offset.saturating_sub(start)).unwrap_or(usize::MAX);
            let to = usize::try_from(end - start)
                .unwrap_or(usize::MAX)
                .min(data.len());
            if from < to {
                out.extend_from_slice(&data[from..to]);
            }
        }
        Ok(out)
    }

    /// Up to `len` bytes at `offset`, read from the backend without the cache.
    ///
    /// # Errors
    ///
    /// The backend failed to read.
    pub async fn read_uncached(&self, offset: u64, len: usize) -> Result<Vec<u8>, VfsError> {
        let len = self.size.saturating_sub(offset).min(len as u64);
        if len == 0 {
            return Ok(Vec::new());
        }
        let mut stream = self.stream_from(offset).await?;
        let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
        (&mut stream)
            .take(len)
            .read_to_end(&mut out)
            .await
            .map_err(|error| VfsError::from_io(error, &self.path))?;
        Ok(out)
    }

    /// The file's contents from `offset` on, as one stream: sought, started there, or skipped to.
    ///
    /// # Errors
    ///
    /// The backend failed to open the file or to seek in it.
    pub async fn stream_from(&self, offset: u64) -> Result<ReadStream, VfsError> {
        let io = |error| VfsError::from_io(error, &self.path);
        if self.seekable.load(Ordering::Relaxed) {
            match self.vfs.open_read_seekable(&self.path).await {
                Ok(mut stream) => {
                    stream.seek(SeekFrom::Start(offset)).await.map_err(io)?;
                    return Ok(Box::new(stream));
                }
                Err(VfsError::Unsupported { .. }) => self.seekable.store(false, Ordering::Relaxed),
                Err(error) => return Err(error),
            }
        }
        if offset > 0 && self.resumable.load(Ordering::Relaxed) {
            match self.vfs.open_read_at(&self.path, offset).await {
                Ok(stream) => return Ok(stream),
                Err(VfsError::Unsupported { .. }) => self.resumable.store(false, Ordering::Relaxed),
                Err(error) => return Err(error),
            }
        }
        let mut stream = self.vfs.open_read(&self.path).await?;
        // A stream that ends before `offset` stays at its end: reads answer nothing.
        tokio::io::copy(&mut (&mut stream).take(offset), &mut tokio::io::sink())
            .await
            .map_err(io)?;
        Ok(stream)
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, BlockCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The most recently used blocks, the latest first.
#[derive(Default)]
struct BlockCache {
    blocks: VecDeque<(u64, Arc<[u8]>)>,
}

impl BlockCache {
    fn get(&mut self, index: u64) -> Option<Arc<[u8]>> {
        let position = self.blocks.iter().position(|(at, _)| *at == index)?;
        let entry = self.blocks.remove(position)?;
        let data = Arc::clone(&entry.1);
        self.blocks.push_front(entry);
        Some(data)
    }

    fn insert(&mut self, index: u64, data: Arc<[u8]>) {
        self.blocks.retain(|(at, _)| *at != index);
        self.blocks.push_front((index, data));
        self.blocks.truncate(CACHE_BLOCKS);
    }

    fn bytes(&self) -> usize {
        self.blocks.iter().map(|(_, data)| data.len()).sum()
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
