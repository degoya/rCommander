//! A file read off a device as a stream: the session's blocking download feeds a bounded channel
//! of chunks, the caller reads them as an [`AsyncRead`]. The download runs at most a few chunks
//! ahead of the reader and stops once the reader is dropped; a download that ends short of the
//! file's size is an error, never a quiet end. While a reader lives it counts as an open download
//! of its device, which streams no upload beside it (`upload.rs`).

use std::{
    io,
    path::Path,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, ready},
};

use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::mpsc,
};

use super::{Session, Sink};
use crate::{ReadStream, Result, VfsError};

/// Chunks the download may be ahead of the reader.
const DEPTH: usize = 4;

type Message = io::Result<Vec<u8>>;

/// One open download in its device's count, until it is dropped.
struct Open(Arc<AtomicUsize>);

impl Open {
    fn new(count: Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count)
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The reader's end.
struct PipeReader {
    rx: mpsc::Receiver<Message>,
    chunk: Vec<u8>,
    offset: usize,
    received: u64,
    /// The file's size: a download that closes short of it failed.
    expected: u64,
    done: bool,
    _open: Open,
}

impl PipeReader {
    fn accept(&mut self, message: Option<Message>) -> io::Result<()> {
        match message {
            Some(Ok(chunk)) => {
                self.received = self.received.saturating_add(chunk.len() as u64);
                self.chunk = chunk;
                self.offset = 0;
                Ok(())
            }
            Some(Err(error)) => {
                self.done = true;
                Err(error)
            }
            None => {
                self.done = true;
                if self.received < self.expected {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "the device stopped before the end of the file",
                    ));
                }
                Ok(())
            }
        }
    }
}

impl AsyncRead for PipeReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if this.offset < this.chunk.len() {
                let count = buf.remaining().min(this.chunk.len() - this.offset);
                buf.put_slice(&this.chunk[this.offset..this.offset + count]);
                this.offset += count;
                return Poll::Ready(Ok(()));
            }
            if this.done {
                return Poll::Ready(Ok(()));
            }
            let message = ready!(this.rx.poll_recv(cx));
            this.accept(message)?;
        }
    }
}

/// The file `id` of `size` bytes at `path`, downloaded by `session` on a blocking thread and
/// counted in `open` while its reader lives. Answers once the first chunk, the end or the error
/// is there, so a file that cannot be read fails here, typed.
pub(super) async fn download(
    session: Arc<dyn Session>,
    id: String,
    size: u64,
    path: &Path,
    open: Arc<AtomicUsize>,
) -> Result<ReadStream> {
    let open = Open::new(open);
    let (tx, rx) = mpsc::channel::<Message>(DEPTH);
    let failed = tx.clone();
    let owned = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let sink: Sink = Box::new(move |chunk: &[u8]| tx.blocking_send(Ok(chunk.to_vec())).is_ok());
        if let Err(error) = session.download(&id, sink) {
            // A reader that has gone needs the error no more.
            let _ = failed.blocking_send(Err(error.on(owned).into_io()));
        }
    });
    let mut reader = PipeReader {
        rx,
        chunk: Vec::new(),
        offset: 0,
        received: 0,
        expected: size,
        done: false,
        _open: open,
    };
    let first = reader.rx.recv().await;
    reader
        .accept(first)
        .map_err(|error| VfsError::from_io(error, path))?;
    Ok(Box::new(reader))
}
