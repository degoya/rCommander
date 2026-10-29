//! The file streams of [`SftpVfs`](super::SftpVfs) as a copy reads and writes them: a
//! russh-sftp `File` whose I/O errors say what they are (RC-060-07).
//!
//! The client reports a closed channel or a stalled request mid-stream as a plain `Other` error
//! that only wraps its own error type. Here such an error becomes the backend's own
//! `connection_lost` or `sftp_timeout` (a [`VfsError`] inside the I/O error, which
//! [`VfsError::from_io`] gets back) and marks the backend closed - so the operations queue knows
//! the connection went, keeps the part file and resumes on a new session. Any other error passes
//! unchanged; the server's own texts stay out of it as before.

use std::{
    io,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

use russh_sftp::client::{error::Error, fs::File};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::error::SftpError;

/// A file of the server, read or written by a transfer.
pub(super) struct TransferFile {
    file: File,
    path: PathBuf,
    /// The backend's "closed" flag, set when the stream finds the session gone.
    lost: Arc<AtomicBool>,
    timeout_secs: u64,
}

impl TransferFile {
    pub(super) fn new(file: File, path: PathBuf, lost: Arc<AtomicBool>, timeout_secs: u64) -> Self {
        Self {
            file,
            path,
            lost,
            timeout_secs,
        }
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
        match Pin::new(&mut this.file).poll_shutdown(context) {
            Poll::Ready(Err(error)) => Poll::Ready(Err(this.typed(error))),
            other => other,
        }
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
