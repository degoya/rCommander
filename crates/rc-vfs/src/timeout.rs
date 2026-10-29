//! A time limit and a stop for every call of a backend that can hang (RC-080-04).
//!
//! A share whose server is gone, a network drive that was disconnected, a drive without its
//! medium: the operating system answers such a call after many seconds, or not at all, and the
//! call holds whatever waits on it - a panel's listing, the window's next command. [`TimeoutFs`]
//! wraps a backend so that every call ends:
//!
//! - **within a time limit** ([`Timeouts`]): a call on one path - `stat`, opening a file, a
//!   rename - gets [`Timeouts::request`]; a listing and a recursive removal, which grow with the
//!   folder, first ask the folder itself within that limit (a share that does not answer is found
//!   at once) and then get [`Timeouts::listing`]. A stream's read, write or seek that stays
//!   pending longer than the request limit fails too (`stream.rs`). The answer is
//!   `drive_timed_out`.
//! - **on request** ([`IoCancel`]): the calls waiting when [`IoCancel::cancel`] is called end
//!   with `cancelled`, the calls after it run as usual.
//!
//! Either way the call returns at once even when the operating system's call goes on: the local
//! backend runs it on a blocking thread, which finishes or fails on its own and whose answer is
//! dropped. Dropping a call's future stops it the same way.
//!
//! A listing sent in batches (`Vfs::list_batches`, RC-060-02) is bounded as `list` is: the
//! folder answers within the request limit, then the whole listing gets the listing limit; the
//! batches pass on unchanged, so a backend that reads in blocks still shows the first ones early.

mod cancel;
mod stream;

use std::{
    future::Future,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime},
};

use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::{
    Capabilities, Details, Entry, LinkKind, PropertyCapabilities, PropertyChange, ReadStream,
    Result, SeekStream, SetAttributes, Vfs, VfsError, WriteMode, WriteStream,
};

pub use cancel::IoCancel;
use stream::Timed;

/// How long a call on one path may wait for its answer unless the caller says otherwise.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a whole listing may take unless the caller says otherwise.
pub const DEFAULT_LISTING_TIMEOUT: Duration = Duration::from_secs(300);

/// The time limits of a [`TimeoutFs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// The longest wait for a call on one path, and for one step of a stream.
    pub request: Duration,
    /// The longest a listing or a recursive removal may take once its folder has answered.
    pub listing: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            request: DEFAULT_REQUEST_TIMEOUT,
            listing: DEFAULT_LISTING_TIMEOUT,
        }
    }
}

/// A backend whose every call ends within its time limit, or when it is cancelled.
pub struct TimeoutFs {
    inner: Arc<dyn Vfs>,
    timeouts: Timeouts,
    cancel: IoCancel,
}

impl TimeoutFs {
    /// `inner` held to `timeouts`, with a switch of its own.
    #[must_use]
    pub fn new(inner: Arc<dyn Vfs>, timeouts: Timeouts) -> Self {
        Self {
            inner,
            timeouts,
            cancel: IoCancel::new(),
        }
    }

    /// The same backend, stopped by `cancel`.
    #[must_use]
    pub fn with_cancel(mut self, cancel: IoCancel) -> Self {
        self.cancel = cancel;
        self
    }

    /// `call` on `path`, ended after `limit` or by the switch.
    async fn bounded<T>(
        &self,
        path: &Path,
        limit: Duration,
        call: impl Future<Output = Result<T>> + Send,
    ) -> Result<T> {
        let (stop, _waiting) = self.cancel.enter();
        tokio::select! {
            biased;
            () = stop => Err(VfsError::Cancelled {
                path: path.to_path_buf(),
            }),
            answer = tokio::time::timeout(limit, call) => answer.unwrap_or_else(|_| {
                Err(VfsError::TimedOut {
                    path: path.to_path_buf(),
                    seconds: limit.as_secs(),
                })
            }),
        }
    }

    /// A call on one path.
    async fn request<T>(
        &self,
        path: &Path,
        call: impl Future<Output = Result<T>> + Send,
    ) -> Result<T> {
        self.bounded(path, self.timeouts.request, call).await
    }

    /// A call that grows with the folder `dir`: the folder answers within the request limit
    /// first. Any answer but silence lets the call run - its own error is the one reported.
    async fn long<T>(&self, dir: &Path, call: impl Future<Output = Result<T>> + Send) -> Result<T> {
        match self.request(dir, self.inner.stat(dir)).await {
            Err(error @ (VfsError::TimedOut { .. } | VfsError::Cancelled { .. })) => Err(error),
            _ => self.bounded(dir, self.timeouts.listing, call).await,
        }
    }

    fn timed<S>(&self, stream: S, path: &Path) -> Timed<S> {
        Timed::new(stream, self.timeouts.request, path.to_path_buf())
    }
}

#[async_trait]
impl Vfs for TimeoutFs {
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    async fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        self.long(dir, self.inner.list(dir)).await
    }

    async fn list_batches(&self, dir: &Path, batches: Sender<Vec<Entry>>) -> Result<()> {
        self.long(dir, self.inner.list_batches(dir, batches)).await
    }

    async fn stat(&self, path: &Path) -> Result<Entry> {
        self.request(path, self.inner.stat(path)).await
    }

    async fn open_read(&self, path: &Path) -> Result<ReadStream> {
        let stream = self.request(path, self.inner.open_read(path)).await?;
        Ok(Box::new(self.timed(stream, path)))
    }

    async fn open_read_at(&self, path: &Path, offset: u64) -> Result<ReadStream> {
        let stream = self
            .request(path, self.inner.open_read_at(path, offset))
            .await?;
        Ok(Box::new(self.timed(stream, path)))
    }

    async fn open_read_seekable(&self, path: &Path) -> Result<SeekStream> {
        let stream = self
            .request(path, self.inner.open_read_seekable(path))
            .await?;
        Ok(Box::new(self.timed(stream, path)))
    }

    async fn open_write(&self, path: &Path, mode: WriteMode) -> Result<WriteStream> {
        let stream = self
            .request(path, self.inner.open_write(path, mode))
            .await?;
        Ok(Box::new(self.timed(stream, path)))
    }

    async fn mkdir(&self, path: &Path, recursive: bool) -> Result<()> {
        self.request(path, self.inner.mkdir(path, recursive)).await
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.request(from, self.inner.rename(from, to)).await
    }

    async fn rename_no_replace(&self, from: &Path, to: &Path) -> Result<()> {
        self.request(from, self.inner.rename_no_replace(from, to))
            .await
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        self.request(path, self.inner.remove_file(path)).await
    }

    async fn remove_dir(&self, path: &Path, recursive: bool) -> Result<()> {
        if recursive {
            self.long(path, self.inner.remove_dir(path, true)).await
        } else {
            self.request(path, self.inner.remove_dir(path, false)).await
        }
    }

    async fn set_mtime(&self, path: &Path, modified: SystemTime) -> Result<()> {
        self.request(path, self.inner.set_mtime(path, modified))
            .await
    }

    async fn set_attributes(&self, path: &Path, change: SetAttributes) -> Result<()> {
        self.request(path, self.inner.set_attributes(path, change))
            .await
    }

    async fn free_space(&self, path: &Path) -> Result<Option<u64>> {
        self.request(path, self.inner.free_space(path)).await
    }

    fn property_capabilities(&self) -> PropertyCapabilities {
        self.inner.property_capabilities()
    }

    async fn details(&self, path: &Path) -> Result<Details> {
        self.request(path, self.inner.details(path)).await
    }

    async fn list_details(&self, dir: &Path) -> Result<Vec<(Entry, Details)>> {
        self.long(dir, self.inner.list_details(dir)).await
    }

    async fn set_properties(&self, path: &Path, change: PropertyChange) -> Result<()> {
        self.request(path, self.inner.set_properties(path, change))
            .await
    }

    async fn create_link(&self, link: &Path, target: &Path, kind: LinkKind) -> Result<()> {
        self.request(link, self.inner.create_link(link, target, kind))
            .await
    }
}

#[cfg(test)]
#[path = "timeout_tests.rs"]
mod tests;
