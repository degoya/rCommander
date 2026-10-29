//! The streams of a [`crate::TimeoutFs`]: a read, a write or a seek that stays pending longer
//! than the time limit fails with `drive_timed_out`, so a copy from a share that went away ends
//! instead of waiting forever.

use std::{
    io,
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf},
    time::Sleep,
};

use crate::VfsError;

/// A stream whose every operation answers within `limit`.
pub(crate) struct Timed<S> {
    inner: S,
    limit: Duration,
    path: PathBuf,
    /// Runs from the moment an operation first stays pending; gone once it is answered.
    deadline: Option<Pin<Box<Sleep>>>,
}

impl<S> Timed<S> {
    pub(crate) fn new(inner: S, limit: Duration, path: PathBuf) -> Self {
        Self {
            inner,
            limit,
            path,
            deadline: None,
        }
    }

    /// `polled` as it is, or `drive_timed_out` once a pending operation has waited `limit`.
    fn watch<T>(
        &mut self,
        context: &mut Context<'_>,
        polled: Poll<io::Result<T>>,
    ) -> Poll<io::Result<T>> {
        if polled.is_ready() {
            self.deadline = None;
            return polled;
        }
        let limit = self.limit;
        let deadline = self
            .deadline
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(limit)));
        match deadline.as_mut().poll(context) {
            Poll::Ready(()) => {
                self.deadline = None;
                let timed_out = VfsError::TimedOut {
                    path: self.path.clone(),
                    seconds: limit.as_secs(),
                };
                Poll::Ready(Err(timed_out.into_io()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Timed<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.inner).poll_read(context, buffer);
        this.watch(context, polled)
    }
}

impl<S: AsyncSeek + Unpin> AsyncSeek for Timed<S> {
    fn start_seek(self: Pin<&mut Self>, position: io::SeekFrom) -> io::Result<()> {
        Pin::new(&mut self.get_mut().inner).start_seek(position)
    }

    fn poll_complete(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.inner).poll_complete(context);
        this.watch(context, polled)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Timed<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.inner).poll_write(context, bytes);
        this.watch(context, polled)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.inner).poll_flush(context);
        this.watch(context, polled)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.inner).poll_shutdown(context);
        this.watch(context, polled)
    }
}
