//! What the tests of the crates that work against [`Vfs`] wrap a backend with (RC-080-24):
//! [`Forward`], a backend that hands every call to an inner one except those it overrides, and
//! [`Trickle`], the local disk with file streams that move slowly. [`archive_name_parts`] cleans
//! an archive's stored name as the archive view does on Windows or elsewhere, whatever system
//! the test runs on.
//!
//! Compiled for this crate's tests and behind the `memory` feature, as [`crate::MemoryFs`] is.

use std::{
    io,
    path::Path,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::mpsc::Sender,
    time::Sleep,
};

use crate::{
    Capabilities, CopyWatch, Entry, LocalFs, ReadStream, Result, SetAttributes, Vfs, VfsError,
    WriteMode, WriteStream,
};

/// A backend that forwards to [`Forward::inner`]: a test overrides only the calls it changes
/// and is a [`Vfs`] through the blanket implementation.
///
/// Only the calls a backend must implement forward, with [`Vfs::list_batches`],
/// [`Vfs::open_read_at`], [`Vfs::open_write_sized`], [`Vfs::rename_replacing`] and
/// [`Vfs::copy_file`] overridable on top; each defaults to what
/// [`Vfs`]'s default does, over this backend's own `list`, `open_read` and `open_write`. Every other provided call keeps [`Vfs`]'s default
/// - a wrapper is exactly the hand-written forward it replaces. So a wrapper streams its files,
/// whatever the inner backend's own system copy (RC-110-65), unless it overrides `copy_file`.
///
/// Implement it by path (`impl rc_vfs::testing::Forward for ...`) without importing it: with
/// both traits in scope a call on the wrapper is ambiguous.
#[async_trait]
pub trait Forward: Send + Sync {
    /// The backend every call not overridden goes to.
    fn inner(&self) -> &dyn Vfs;

    fn capabilities(&self) -> Capabilities {
        self.inner().capabilities()
    }
    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        self.inner().list(dir).await
    }
    async fn list_batches(&self, dir: &Path, batches: Sender<Vec<Entry>>) -> Result<()> {
        let entries = Forward::list(self, dir).await?;
        // A receiver that is gone wants nothing more.
        let _ = batches.send(entries).await;
        Ok(())
    }
    async fn stat(&self, path: &Path) -> Result<Entry> {
        self.inner().stat(path).await
    }
    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        self.inner().open_read(path).await
    }
    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        if offset == 0 {
            return Forward::open_read(self, path).await;
        }
        Err(VfsError::Unsupported {
            path: path.to_path_buf(),
            operation: "reading from an offset",
        })
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        self.inner().open_write(path, mode).await
    }
    async fn open_write_sized(
        &self,
        path: &Path,
        mode: WriteMode,
        _size: u64,
    ) -> Result<WriteStream> {
        Forward::open_write(self, path, mode).await
    }
    async fn mkdir(&self, path: &Path, recursive: bool) -> Result<()> {
        self.inner().mkdir(path, recursive).await
    }
    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.inner().rename(from, to).await
    }
    /// The wrapper's own [`Forward::rename`] unless overridden, so a wrapper that refuses or
    /// counts renames sees this one too.
    async fn rename_replacing(&self, from: &Path, to: &Path) -> Result<()> {
        Forward::rename(self, from, to).await
    }
    async fn remove_file(&self, path: &Path) -> Result<()> {
        self.inner().remove_file(path).await
    }
    async fn remove_dir(&self, path: &Path, recursive: bool) -> Result<()> {
        self.inner().remove_dir(path, recursive).await
    }
    async fn set_mtime(&self, path: &Path, modified: SystemTime) -> Result<()> {
        self.inner().set_mtime(path, modified).await
    }
    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> Result<()> {
        self.inner().set_attributes(path, change).await
    }
    async fn sync(&self, path: &Path) -> Result<()> {
        self.inner().sync(path).await
    }
    async fn copy_file(&self, from: &Path, _to: &Path, _watch: Arc<CopyWatch>) -> Result<u64> {
        Err(VfsError::Unsupported {
            path: from.to_path_buf(),
            operation: "copying through the operating system",
        })
    }
}

#[async_trait]
impl<T: Forward> Vfs for T {
    fn capabilities(&self) -> Capabilities {
        Forward::capabilities(self)
    }
    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        Forward::list(self, dir).await
    }
    async fn list_batches(&self, dir: &Path, batches: Sender<Vec<Entry>>) -> Result<()> {
        Forward::list_batches(self, dir, batches).await
    }
    async fn stat(&self, path: &Path) -> Result<Entry> {
        Forward::stat(self, path).await
    }
    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        Forward::open_read(self, path).await
    }
    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        Forward::open_read_at(self, path, offset).await
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        Forward::open_write(self, path, mode).await
    }
    async fn open_write_sized(
        &self,
        path: &Path,
        mode: WriteMode,
        size: u64,
    ) -> Result<WriteStream> {
        Forward::open_write_sized(self, path, mode, size).await
    }
    async fn mkdir(&self, path: &Path, recursive: bool) -> Result<()> {
        Forward::mkdir(self, path, recursive).await
    }
    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        Forward::rename(self, from, to).await
    }
    async fn rename_replacing(&self, from: &Path, to: &Path) -> Result<()> {
        Forward::rename_replacing(self, from, to).await
    }
    async fn remove_file(&self, path: &Path) -> Result<()> {
        Forward::remove_file(self, path).await
    }
    async fn remove_dir(&self, path: &Path, recursive: bool) -> Result<()> {
        Forward::remove_dir(self, path, recursive).await
    }
    async fn set_mtime(&self, path: &Path, modified: SystemTime) -> Result<()> {
        Forward::set_mtime(self, path, modified).await
    }
    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> Result<()> {
        Forward::set_attributes(self, path, change).await
    }
    async fn sync(&self, path: &Path) -> Result<()> {
        Forward::sync(self, path).await
    }
    async fn copy_file(&self, from: &Path, to: &Path, watch: Arc<CopyWatch>) -> Result<u64> {
        Forward::copy_file(self, from, to, watch).await
    }
}

/// The wait before each read of a [`Trickle`] stream.
pub const TRICKLE_PAUSE: Duration = Duration::from_millis(15);
/// The most one read of a [`Trickle`] stream moves.
pub const TRICKLE_CHUNK: usize = 64 * 1024;

/// [`LocalFs`] whose file streams trickle (RC-080-04): every read waits [`TRICKLE_PAUSE`] and
/// moves at most [`TRICKLE_CHUNK`] bytes, so a copy of a few MiB is caught in its middle.
#[derive(Default)]
pub struct Trickle(pub LocalFs);

#[async_trait]
impl Forward for Trickle {
    fn inner(&self) -> &dyn Vfs {
        &self.0
    }
    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        let inner = self.0.open_read(path).await?;
        Ok(Box::new(Slowly { inner, wait: None }))
    }
}

/// A stream that waits [`TRICKLE_PAUSE`] before each read of at most [`TRICKLE_CHUNK`] bytes.
struct Slowly {
    inner: ReadStream,
    wait: Option<Pin<Box<Sleep>>>,
}

impl AsyncRead for Slowly {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let wait = this
            .wait
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(TRICKLE_PAUSE)));
        ready!(wait.as_mut().poll(context));
        let mut bytes = vec![0_u8; buffer.remaining().min(TRICKLE_CHUNK)];
        let mut part = ReadBuf::new(&mut bytes);
        ready!(Pin::new(&mut this.inner).poll_read(context, &mut part))?;
        buffer.put_slice(part.filled());
        this.wait = None;
        Poll::Ready(Ok(()))
    }
}

/// The parts the archive view lists a stored entry `name` under on a system that keeps Windows'
/// rules (`windows`) or not (RC-110-92): what an unpacking or a copy out of the archive joins
/// below its target.
#[must_use]
pub fn archive_name_parts(name: &str, windows: bool) -> Vec<String> {
    crate::archive::safety::clean_name_for(name, windows).parts
}

#[cfg(test)]
#[path = "testing_tests.rs"]
mod tests;
