//! A server with limited bandwidth (RC-060-06): an in-memory backend whose streams take
//! `bytes / rate` for every read and every write, each from the moment it is asked for - nothing
//! saved up while the stream waits - so a copy that reads and writes in turn takes the sum of
//! both sides, one that does both at once the longer of them. Measured on tokio's clock, which a
//! test may pause.

use std::{
    io,
    path::Path,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};

use async_trait::async_trait;
use rc_vfs::{MemoryFs, ReadStream, Vfs, WriteMode, WriteStream};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::Sleep,
};

#[derive(Clone)]
pub struct Slow {
    pub fs: MemoryFs,
    /// Bytes per second, each way.
    pub rate: u64,
}

/// How long `bytes` take at `rate` bytes a second.
fn cost(bytes: usize, rate: u64) -> Duration {
    Duration::from_nanos((bytes as u64).saturating_mul(1_000_000_000) / rate.max(1))
}

/// A stream and the request it is waiting out: the wait and how many bytes it is for.
struct Throttled<S> {
    inner: S,
    rate: u64,
    waiting: Option<(Pin<Box<Sleep>>, usize)>,
}

impl<S> Throttled<S> {
    fn new(inner: S, rate: u64) -> Self {
        Self {
            inner,
            rate,
            waiting: None,
        }
    }

    /// Waits out a request of up to `asked` bytes; how many it may move.
    fn poll_turn(&mut self, context: &mut Context<'_>, asked: usize) -> Poll<usize> {
        let rate = self.rate;
        let (sleep, granted) = self
            .waiting
            .get_or_insert_with(|| (Box::pin(tokio::time::sleep(cost(asked, rate))), asked));
        ready!(sleep.as_mut().poll(context));
        let granted = (*granted).min(asked);
        self.waiting = None;
        Poll::Ready(granted)
    }
}

impl AsyncRead for Throttled<ReadStream> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let granted = ready!(this.poll_turn(context, buffer.remaining()));
        let mut bytes = vec![0_u8; granted];
        let mut part = ReadBuf::new(&mut bytes);
        // The memory stream below is always ready.
        ready!(Pin::new(&mut this.inner).poll_read(context, &mut part))?;
        buffer.put_slice(part.filled());
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for Throttled<WriteStream> {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let granted = ready!(this.poll_turn(context, bytes.len()));
        Pin::new(&mut this.inner).poll_write(context, &bytes[..granted])
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for Slow {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        let inner = self.fs.open_read(path).await?;
        Ok(Box::new(Throttled::new(inner, self.rate)))
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        let inner = self.fs.open_write(path, mode).await?;
        Ok(Box::new(Throttled::new(inner, self.rate)))
    }
}
