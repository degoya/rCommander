//! The test servers' TCP stream, which [`super::TestServer::freeze`] and
//! [`crate::testkit::ShellServer::freeze`] make go silent.

use std::{
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};

/// A TCP stream that can go silent.
pub(in crate::testkit) struct Link {
    pub(in crate::testkit) stream: TcpStream,
    pub(in crate::testkit) frozen: Arc<AtomicBool>,
}

impl Link {
    fn frozen(&self) -> bool {
        self.frozen.load(Ordering::SeqCst)
    }
}

impl AsyncRead for Link {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.frozen() {
            // Silence for good: nothing arrives, nothing wakes the reader again.
            return Poll::Pending;
        }
        Pin::new(&mut self.get_mut().stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for Link {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.frozen() {
            return Poll::Ready(Ok(buf.len()));
        }
        Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.frozen() {
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.frozen() {
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}
