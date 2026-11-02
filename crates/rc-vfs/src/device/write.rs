//! A file written onto a device. MTP takes a file whole and wants its size before the first
//! byte, which a stream does not know: the bytes go into an anonymous temporary file of this
//! machine first (deleted by the system when closed), and `shutdown` hands it to the device in
//! one upload. Nothing is on the device before that; a stream dropped before or during the
//! upload stops it, and a stopped upload leaves nothing behind. A file it replaces is deleted only
//! once the new one is complete.

use std::{
    future::Future,
    io::{self, Seek, SeekFrom},
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, ready},
};

use tokio::{io::AsyncWrite, task::JoinHandle};

use super::{Session, cache::ObjectCache};
use crate::{Result, VfsError, WriteStream};

/// Where a written file goes once it is complete.
pub(super) struct Target {
    pub session: Arc<dyn Session>,
    pub cache: Arc<Mutex<ObjectCache>>,
    /// The folder's object id.
    pub folder: String,
    pub name: String,
    /// The file's path, for its errors and the cache.
    pub path: PathBuf,
    /// The object the new file replaces, deleted after the upload.
    pub replaced: Option<String>,
}

impl Target {
    /// Uploads the `size` bytes of `file`, then deletes the file it replaces.
    fn upload(self, mut file: std::fs::File, size: u64, cancel: Arc<AtomicBool>) -> Result<()> {
        file.seek(SeekFrom::Start(0))
            .map_err(|error| VfsError::from_io(error, &self.path))?;
        let object = self
            .session
            .upload(&self.folder, &self.name, size, Box::new(file), cancel)
            .map_err(|error| error.on(&self.path))?;
        if let Some(old) = &self.replaced
            && let Err(error) = self.session.delete(old, false)
        {
            // The new file is complete; the old one stays beside it under the same name.
            let error = error.on(&self.path);
            tracing::warn!(
                path = %self.path.display(),
                code = error.code(),
                "the file a copy replaced on the device could not be deleted: {error}"
            );
        }
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .put(self.path, object);
        Ok(())
    }
}

enum State {
    /// Bytes go into the temporary file; `written` so far.
    Spooling { file: tokio::fs::File, written: u64 },
    /// The device takes the file.
    Uploading(JoinHandle<Result<()>>),
    /// Uploaded, or failed.
    Closed,
}

/// The stream [`super::DeviceFs::open_write`] answers.
struct SpoolWriter {
    state: State,
    target: Option<Target>,
    /// Set when the stream is dropped: an upload still running stops.
    cancel: Arc<AtomicBool>,
}

impl Drop for SpoolWriter {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// A stream that writes `target` once it is shut down.
pub(super) fn spool(target: Target) -> Result<WriteStream> {
    let file = tempfile::tempfile().map_err(|error| VfsError::from_io(error, &target.path))?;
    Ok(Box::new(SpoolWriter {
        state: State::Spooling {
            file: tokio::fs::File::from_std(file),
            written: 0,
        },
        target: Some(target),
        cancel: Arc::new(AtomicBool::new(false)),
    }))
}

impl SpoolWriter {
    /// Hands the spooled `file` of `size` bytes to the device on a blocking thread.
    fn start(&mut self, file: tokio::fs::File, size: u64) -> io::Result<JoinHandle<Result<()>>> {
        // Flushed, so no operation of the file is in flight.
        let file = file
            .try_into_std()
            .map_err(|_| io::Error::other("the temporary file is still busy"))?;
        let target = self.target.take().ok_or_else(closed)?;
        let cancel = Arc::clone(&self.cancel);
        Ok(tokio::task::spawn_blocking(move || {
            target.upload(file, size, cancel)
        }))
    }
}

fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "the device file is closed")
}

impl AsyncWrite for SpoolWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match &mut this.state {
            State::Spooling { file, written } => {
                let count = ready!(Pin::new(file).poll_write(cx, buf))?;
                *written = written.saturating_add(count as u64);
                Poll::Ready(Ok(count))
            }
            State::Uploading(_) | State::Closed => Poll::Ready(Err(closed())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().state {
            State::Spooling { file, .. } => Pin::new(file).poll_flush(cx),
            State::Uploading(_) | State::Closed => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            match &mut this.state {
                State::Spooling { file, written } => {
                    ready!(Pin::new(&mut *file).poll_flush(cx))?;
                    let size = *written;
                    if let State::Spooling { file, .. } =
                        std::mem::replace(&mut this.state, State::Closed)
                    {
                        this.state = State::Uploading(this.start(file, size)?);
                    }
                }
                State::Uploading(task) => {
                    let joined = ready!(Pin::new(task).poll(cx));
                    this.state = State::Closed;
                    return Poll::Ready(match joined {
                        Ok(uploaded) => uploaded.map_err(VfsError::into_io),
                        Err(error) => Err(io::Error::other(error)),
                    });
                }
                State::Closed => return Poll::Ready(Ok(())),
            }
        }
    }
}
