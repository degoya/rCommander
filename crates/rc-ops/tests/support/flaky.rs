//! A server whose connection breaks (RC-060-07): an in-memory backend whose streams fail with a
//! broken link (or another `fault`, RC-060-06) after a number of bytes, a set number of times,
//! and count what went through.
//! Without `offsets` it reads from the start only, as a server without `REST`.

use std::{
    io,
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll},
};

use async_trait::async_trait;
use rc_vfs::{MemoryFs, ReadStream, Vfs, VfsError, WriteMode, WriteStream};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// When the link breaks, and what went through it.
#[derive(Debug, Default)]
pub struct Link {
    /// A write stream breaks once this many bytes went through it...
    pub break_writes_after: Option<u64>,
    /// ... this many times.
    pub write_breaks: u32,
    /// A read stream breaks once this many bytes went through it...
    pub break_reads_after: Option<u64>,
    /// ... this many times.
    pub read_breaks: u32,
    /// Bytes written through every stream.
    pub written: u64,
    /// Bytes read through every stream.
    pub read: u64,
    /// Reads from an offset work (`REST`).
    pub offsets: bool,
    /// What a break is instead of a broken link: a failure that is no lost connection.
    pub fault: Option<io::ErrorKind>,
    /// Opening a file to write is refused, as a read-only folder does (RC-060-06).
    pub refuse_writes: bool,
    /// A rename is refused, as a folder that takes files but no renames does.
    pub refuse_renames: bool,
}

/// The refusal of a folder that may not be changed.
fn denied(path: &Path) -> VfsError {
    VfsError::PermissionDenied {
        path: path.to_path_buf(),
    }
}

/// The broken link of a stream that went past its limit, or the `fault` set instead.
fn broken(kind: io::ErrorKind, fault: Option<io::ErrorKind>) -> io::Error {
    io::Error::new(fault.unwrap_or(kind), "the test link broke")
}

#[derive(Clone)]
pub struct Flaky {
    pub fs: MemoryFs,
    pub link: Arc<Mutex<Link>>,
}

impl Flaky {
    pub fn new(fs: MemoryFs) -> Self {
        Self {
            fs,
            link: Arc::new(Mutex::new(Link {
                offsets: true,
                ..Link::default()
            })),
        }
    }

    pub fn link(&self) -> MutexGuard<'_, Link> {
        self.link.lock().expect("link")
    }
}

struct Reader {
    inner: ReadStream,
    link: Arc<Mutex<Link>>,
    through: u64,
}

impl AsyncRead for Reader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        {
            let mut link = self.link.lock().expect("link");
            if link.read_breaks > 0
                && link
                    .break_reads_after
                    .is_some_and(|limit| self.through >= limit)
            {
                link.read_breaks -= 1;
                return Poll::Ready(Err(broken(io::ErrorKind::ConnectionReset, link.fault)));
            }
        }
        let before = buffer.filled().len();
        let polled = Pin::new(&mut self.inner).poll_read(context, buffer);
        let count = (buffer.filled().len() - before) as u64;
        self.through += count;
        self.link.lock().expect("link").read += count;
        polled
    }
}

struct Writer {
    inner: WriteStream,
    link: Arc<Mutex<Link>>,
    through: u64,
}

impl AsyncWrite for Writer {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut take = bytes.len();
        {
            let mut link = self.link.lock().expect("link");
            if link.write_breaks > 0
                && let Some(limit) = link.break_writes_after
            {
                if self.through >= limit {
                    link.write_breaks -= 1;
                    return Poll::Ready(Err(broken(io::ErrorKind::ConnectionAborted, link.fault)));
                }
                take = take.min(usize::try_from(limit - self.through).unwrap_or(usize::MAX));
            }
        }
        let polled = Pin::new(&mut self.inner).poll_write(context, &bytes[..take]);
        if let Poll::Ready(Ok(count)) = &polled {
            self.through += *count as u64;
            self.link.lock().expect("link").written += *count as u64;
        }
        polled
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

#[async_trait]
impl rc_vfs::testing::Forward for Flaky {
    fn inner(&self) -> &dyn Vfs {
        &self.fs
    }
    async fn open_read(&self, path: &Path) -> rc_vfs::Result<ReadStream> {
        rc_vfs::testing::Forward::open_read_at(self, path, 0).await
    }
    async fn open_read_at(&self, path: &Path, offset: u64) -> rc_vfs::Result<ReadStream> {
        if offset > 0 && !self.link().offsets {
            return Err(VfsError::Unsupported {
                path: path.to_path_buf(),
                operation: "REST",
            });
        }
        Ok(Box::new(Reader {
            inner: self.fs.open_read_at(path, offset).await?,
            link: Arc::clone(&self.link),
            through: 0,
        }))
    }
    async fn open_write(&self, path: &Path, mode: WriteMode) -> rc_vfs::Result<WriteStream> {
        if self.link().refuse_writes {
            return Err(denied(path));
        }
        Ok(Box::new(Writer {
            inner: self.fs.open_write(path, mode).await?,
            link: Arc::clone(&self.link),
            through: 0,
        }))
    }
    async fn rename(&self, from: &Path, to: &Path) -> rc_vfs::Result<()> {
        if self.link().refuse_renames {
            return Err(denied(to));
        }
        self.fs.rename(from, to).await
    }
}

/// Waits until `check` holds, for at most two seconds.
pub async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    for _ in 0..400 {
        if check() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("never: {what}");
}
