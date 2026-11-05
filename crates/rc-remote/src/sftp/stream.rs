//! The file streams of [`SftpVfs`](super::SftpVfs) as a copy reads and writes them: a
//! russh-sftp `File` whose I/O errors say what they are (RC-060-07).
//!
//! The client reports a closed channel or a stalled request mid-stream as a plain `Other` error
//! that only wraps its own error type. Here such an error becomes the backend's own
//! `connection_lost` or `sftp_timeout` (a [`VfsError`] inside the I/O error, which
//! [`VfsError::from_io`] gets back) and marks the backend closed - so the operations queue knows
//! the connection went, keeps the part file and resumes on a new session. Any other error passes
//! unchanged; the server's own texts stay out of it as before.
//!
//! An upload that created its file owner-only gives it its rights when it shuts down cleanly
//! ([`Settle`], RC-080-67); one that breaks off leaves the part owner-only.

use std::{
    future::Future,
    io,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, ready},
};

use russh_sftp::{
    client::{RawSftpSession, error::Error, fs::File},
    protocol::FileAttributes,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::error::SftpError;

/// What a completed upload still owes its file: the rights it gets once it is whole.
pub(super) struct Settle {
    pub raw: Arc<RawSftpSession>,
    pub wire: String,
    pub mode: u32,
}

impl Settle {
    /// `SETSTAT` with the rights; a refusal leaves the file owner-only, the safe side.
    async fn run(self) {
        let change = FileAttributes {
            permissions: Some(self.mode),
            ..FileAttributes::empty()
        };
        if self.raw.setstat(self.wire, change).await.is_err() {
            tracing::debug!("the server kept an upload owner-only");
        }
    }
}

/// A file of the server, read or written by a transfer.
pub(super) struct TransferFile {
    file: File,
    path: PathBuf,
    /// The backend's "closed" flag, set when the stream finds the session gone.
    lost: Arc<AtomicBool>,
    timeout_secs: u64,
    /// The rights to give an upload's file after its clean close.
    settle: Option<Settle>,
    settling: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
    /// The file's own close is done; a later poll only finishes the settling.
    closed: bool,
}

impl TransferFile {
    pub(super) fn new(file: File, path: PathBuf, lost: Arc<AtomicBool>, timeout_secs: u64) -> Self {
        Self {
            file,
            path,
            lost,
            timeout_secs,
            settle: None,
            settling: None,
            closed: false,
        }
    }

    /// The stream with `settle` to run after its clean close.
    pub(super) fn settling_with(mut self, settle: Option<Settle>) -> Self {
        self.settle = settle;
        self
    }

    /// `error` typed: a lost or stalled session as the backend's error, anything else as it is.
    fn typed(&self, error: io::Error) -> io::Error {
        let failure = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<Error>())
            .and_then(|inner| SftpError::of_client(inner, self.timeout_secs));
        typed_failure(failure, &self.path, &self.lost).unwrap_or(error)
    }
}

/// The I/O error of a session failure that ends the stream: `connection_lost` or `sftp_timeout`,
/// with the backend marked closed; `None` for anything else.
pub(super) fn typed_failure(
    failure: Option<SftpError>,
    path: &std::path::Path,
    lost: &AtomicBool,
) -> Option<io::Error> {
    match failure {
        Some(failure @ (SftpError::ConnectionLost | SftpError::Timeout { .. })) => {
            lost.store(true, Ordering::SeqCst);
            Some(failure.into_vfs(path).into_io())
        }
        _ => None,
    }
}

impl AsyncRead for TransferFile {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Pin::new(&mut this.file).poll_read(context, buffer) {
            Poll::Ready(Err(error)) => Poll::Ready(Err(this.typed(error))),
            other => other,
        }
    }
}

impl AsyncWrite for TransferFile {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.file).poll_write(context, bytes) {
            Poll::Ready(Err(error)) => Poll::Ready(Err(this.typed(error))),
            other => other,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Pin::new(&mut this.file).poll_flush(context) {
            Poll::Ready(Err(error)) => Poll::Ready(Err(this.typed(error))),
            other => other,
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.closed {
            match ready!(Pin::new(&mut this.file).poll_shutdown(context)) {
                Ok(()) => this.closed = true,
                Err(error) => return Poll::Ready(Err(this.typed(error))),
            }
        }
        if let Some(settle) = this.settle.take() {
            this.settling = Some(Box::pin(settle.run()));
        }
        if let Some(settling) = this.settling.as_mut() {
            ready!(settling.as_mut().poll(context));
            this.settling = None;
        }
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
