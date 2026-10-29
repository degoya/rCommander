//! From a blocking producer (an archive crate unpacking one entry) to the async [`ReadStream`]
//! a caller reads: a bounded channel of chunks. The producer runs at most `DEPTH` chunks ahead
//! of the reader and stops as soon as the reader is dropped, so an entry streams - also from the
//! middle of a solid 7z block - instead of unpacking into memory. A [`Watch`] tells the archive
//! reads underneath that the reader is gone, so a producer still unpacking its way towards the
//! entry stops there too (RC-080-46).

use std::{
    io::{self, Read},
    path::Path,
    pin::Pin,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, ready},
};

use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::mpsc,
};

use super::source::io_error;
use crate::{ReadStream, Result, VfsError};

/// Bytes per chunk.
pub(crate) const CHUNK: usize = 64 * 1024;
/// Chunks the producer may be ahead.
const DEPTH: usize = 4;

type Message = io::Result<Vec<u8>>;

/// How a producer ended without an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pumped {
    /// Every byte went out.
    Complete,
    /// The reader went away after `sent` bytes; the producer stopped there.
    Abandoned { sent: u64 },
}

/// The producer's end.
pub(crate) struct PipeWriter {
    tx: mpsc::Sender<Message>,
    /// Set once the reader is dropped.
    gone: Arc<AtomicBool>,
}

impl PipeWriter {
    /// Sends a chunk; `false` when the reader has gone.
    fn send(&self, chunk: Vec<u8>) -> bool {
        self.tx.blocking_send(Ok(chunk)).is_ok()
    }

    /// Ends the stream with an error. A reader that has gone needs it no more.
    pub(crate) fn fail(&self, error: VfsError) {
        let _ = self.tx.blocking_send(Err(error.into_io()));
    }
}

/// The reader's end: an [`AsyncRead`] over the chunks.
pub(crate) struct PipeReader {
    rx: mpsc::Receiver<Message>,
    chunk: Vec<u8>,
    offset: usize,
    received: u64,
    /// The entry's size: a stream that closes short of it is an error, not an end.
    expected: u64,
    done: bool,
    gone: Arc<AtomicBool>,
}

impl Drop for PipeReader {
    fn drop(&mut self) {
        self.gone.store(true, Ordering::Relaxed);
    }
}

/// A connected pair for an entry of `expected` bytes.
pub(crate) fn pair(expected: u64) -> (PipeWriter, PipeReader) {
    let (tx, rx) = mpsc::channel(DEPTH);
    let gone = Arc::new(AtomicBool::new(false));
    let reader = PipeReader {
        rx,
        chunk: Vec::new(),
        offset: 0,
        received: 0,
        expected,
        done: false,
        gone: Arc::clone(&gone),
    };
    (PipeWriter { tx, gone }, reader)
}

/// The stream of `reader` once the first chunk, the end or the error has arrived: an entry that
/// cannot be read fails here, typed.
pub(crate) async fn opened(mut reader: PipeReader, path: &Path) -> Result<ReadStream> {
    let first = reader.rx.recv().await;
    reader
        .accept(first)
        .map_err(|error| VfsError::from_io(error, path))?;
    Ok(Box::new(reader))
}

/// Runs `work` as a blocking task feeding a pipe, and returns the stream as [`opened`] does.
#[cfg(test)]
pub(crate) async fn spawn<F>(path: &Path, expected: u64, work: F) -> Result<ReadStream>
where
    F: FnOnce(&PipeWriter) -> Result<Pumped> + Send + 'static,
{
    let (writer, reader) = pair(expected);
    tokio::task::spawn_blocking(move || {
        if let Err(error) = work(&writer) {
            writer.fail(error);
        }
    });
    opened(reader, path).await
}

/// Which reader the archive reads of a producer work for: once it is dropped, they fail, so a
/// producer unpacking its way towards an entry nobody waits for any more stops instead of
/// decompressing on to the end. Empty, it never stops a read.
#[derive(Clone, Default)]
pub(crate) struct Watch(Arc<Mutex<Option<Arc<AtomicBool>>>>);

impl Watch {
    /// From now on the reads work for the reader of `writer`.
    pub(crate) fn follow(&self, writer: &PipeWriter) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&writer.gone));
    }

    /// An error once the reader followed is gone.
    pub(crate) fn check(&self) -> io::Result<()> {
        let gone = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|gone| gone.load(Ordering::Relaxed));
        if gone {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the reader of the archive entry went away",
            ));
        }
        Ok(())
    }
}

impl PipeReader {
    fn accept(&mut self, message: Option<Message>) -> io::Result<()> {
        match message {
            Some(Ok(chunk)) => {
                self.received += chunk.len() as u64;
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
                    // The producer ended without a word: it panicked.
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "the archive reader stopped before the end of the entry",
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

/// Copies an entry of `declared` bytes from `reader` into the pipe. More bytes than declared is
/// an archive limit - a stated size that lies is what a bomb does - and fewer is damage.
pub(crate) fn pump(
    reader: &mut dyn Read,
    declared: u64,
    writer: &PipeWriter,
    path: &Path,
) -> Result<Pumped> {
    let mut sent: u64 = 0;
    // One buffer for every read; each chunk sent is as large as what was read.
    let mut buffer = vec![0; CHUNK];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(error, path)),
        };
        sent = sent.saturating_add(read as u64);
        if sent > declared {
            return Err(VfsError::ArchiveLimit {
                path: path.to_path_buf(),
                limit: "entry larger than its stated size",
            });
        }
        if !writer.send(buffer[..read].to_vec()) {
            return Ok(Pumped::Abandoned { sent });
        }
    }
    if sent < declared {
        return Err(VfsError::ArchiveDamaged {
            path: path.to_path_buf(),
            reason: "entry shorter than its stated size".to_owned(),
        });
    }
    Ok(Pumped::Complete)
}

#[cfg(test)]
#[path = "pipe_tests.rs"]
mod tests;
