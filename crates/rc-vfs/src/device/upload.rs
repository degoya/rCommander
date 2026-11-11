//! A file streamed onto a device whose size is known before its first byte (RC-110-16). The
//! upload starts at once on a blocking thread and reads its blocks from a bounded channel that the
//! stream's writes fill: a write returns once there is room, so the device has taken all but the
//! last [`DEPTH`] blocks of what was written - the copy's progress is the upload's - and no
//! temporary file is made. The upload takes the device's turn (`DeviceFs::call`) like any call:
//! a listing meanwhile waits for it, as it would behind the device's one thread anyway.
//!
//! The stream takes exactly the bytes it was opened with: one more is an error of the write, fewer
//! an error of `shutdown`. Only a `shutdown` after the last byte ends the upload's reader cleanly,
//! so only then does the device keep the file; a stream that fails, is cut short or is dropped
//! ends the reader with an error, and the upload is reverted, leaving nothing on the device.

use std::{
    future::Future,
    io::{self, Read},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, ready},
};

use tokio::{
    io::AsyncWrite,
    sync::{
        Mutex,
        mpsc::{self, OwnedPermit, error::SendError},
    },
    task::JoinHandle,
};

use super::write::{Target, closed};
use crate::recover::LogPanic as _;
use crate::{Result, VfsError, WriteStream};

/// Blocks the stream may be ahead of the device.
pub(super) const DEPTH: usize = 4;

/// What the stream sends the upload.
enum Block {
    Bytes(Vec<u8>),
    /// Every byte was sent: the file is complete.
    End,
}

/// The upload's source: the blocks as the stream sends them, on the device's thread.
struct Blocks {
    rx: mpsc::Receiver<Block>,
    block: Vec<u8>,
    offset: usize,
    ended: bool,
}

impl Read for Blocks {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.offset < self.block.len() {
                let count = buffer.len().min(self.block.len() - self.offset);
                buffer[..count].copy_from_slice(&self.block[self.offset..self.offset + count]);
                self.offset += count;
                return Ok(count);
            }
            if self.ended {
                return Ok(0);
            }
            match self.rx.blocking_recv() {
                Some(Block::Bytes(bytes)) => {
                    self.block = bytes;
                    self.offset = 0;
                }
                Some(Block::End) => self.ended = true,
                // The stream went without its end: the file is not complete.
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "the copy stopped before the end of the file",
                    ));
                }
            }
        }
    }
}

type Reserve =
    Pin<Box<dyn Future<Output = std::result::Result<OwnedPermit<Block>, SendError<()>>> + Send>>;

enum State {
    /// Blocks go to the upload; `room` is a send waiting for space in the channel.
    Open {
        tx: mpsc::Sender<Block>,
        room: Option<Reserve>,
    },
    /// Nothing more is sent; the upload's answer is awaited. `failure` is the stream's own
    /// error, which is the answer when there is one.
    Closing {
        failure: Option<io::Error>,
    },
    Closed,
}

/// The stream [`stream`] answers.
struct UploadWriter {
    state: State,
    upload: Option<JoinHandle<Result<()>>>,
    size: u64,
    written: u64,
    path: PathBuf,
    /// Set when the stream is dropped: the upload stops before its next block.
    cancel: Arc<AtomicBool>,
}

impl Drop for UploadWriter {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// A stream that uploads `size` bytes to `target` as they are written, once it has `turn`.
pub(super) fn stream(target: Target, size: u64, turn: Arc<Mutex<()>>) -> WriteStream {
    let (tx, rx) = mpsc::channel(DEPTH);
    let cancel = Arc::new(AtomicBool::new(false));
    let path = target.path.clone();
    let source = Blocks {
        rx,
        block: Vec::new(),
        offset: 0,
        ended: false,
    };
    let (stop, failed_on) = (Arc::clone(&cancel), path.clone());
    let upload = tokio::spawn(async move {
        let held = turn.lock_owned().await;
        let ran = tokio::task::spawn_blocking(move || {
            let uploaded = if stop.load(Ordering::SeqCst) {
                // Dropped while it waited for the turn: the device is not asked at all.
                Err(VfsError::Cancelled {
                    path: target.path.clone(),
                })
            } else {
                target.upload(Box::new(source), size, stop)
            };
            drop(held);
            uploaded
        })
        .await
        .log_panic("vfs.device_upload");
        ran.unwrap_or_else(|error| {
            Err(VfsError::Io {
                path: failed_on,
                source: io::Error::other(error),
            })
        })
    });
    Box::new(UploadWriter {
        state: State::Open { tx, room: None },
        upload: Some(upload),
        size,
        written: 0,
        path,
        cancel,
    })
}

fn too_long(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "{}: more bytes than the size the file was announced with",
            path.display()
        ),
    )
}

fn too_short(path: &Path, written: u64, size: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::UnexpectedEof,
        format!(
            "{}: {written} of the {size} bytes the file was announced with",
            path.display()
        ),
    )
}

impl UploadWriter {
    /// The upload's answer, once it has ended.
    fn poll_upload(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let Some(upload) = &mut self.upload else {
            return Poll::Ready(Err(closed()));
        };
        let joined = ready!(Pin::new(upload).poll(cx)).log_panic("vfs.device_upload");
        self.upload = None;
        Poll::Ready(match joined {
            Ok(uploaded) => uploaded.map_err(VfsError::into_io),
            Err(error) => Err(io::Error::other(error)),
        })
    }

    /// Waits for room in the channel; `None` once the upload has stopped reading.
    fn poll_room(&mut self, cx: &mut Context<'_>) -> Poll<Option<OwnedPermit<Block>>> {
        let State::Open { tx, room } = &mut self.state else {
            return Poll::Ready(None);
        };
        let waiting = room.get_or_insert_with(|| Box::pin(tx.clone().reserve_owned()));
        let reserved = ready!(waiting.as_mut().poll(cx));
        *room = None;
        Poll::Ready(reserved.ok())
    }
}

impl AsyncWrite for UploadWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match this.state {
            State::Open { .. } => {}
            // The upload stopped reading: its own error is the answer.
            State::Closing { failure: None } if this.upload.is_some() => {
                let answer = ready!(this.poll_upload(cx));
                return Poll::Ready(Err(answer.err().unwrap_or_else(closed)));
            }
            State::Closing { .. } | State::Closed => return Poll::Ready(Err(closed())),
        }
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let length = buf.len() as u64;
        if length > this.size.saturating_sub(this.written) {
            // Nothing more is sent: the upload ends short and is reverted.
            this.state = State::Closing { failure: None };
            return Poll::Ready(Err(too_long(&this.path)));
        }
        match ready!(this.poll_room(cx)) {
            Some(permit) => {
                drop(permit.send(Block::Bytes(buf.to_vec())));
                this.written += length;
                Poll::Ready(Ok(buf.len()))
            }
            None => {
                this.state = State::Closing { failure: None };
                let answer = ready!(this.poll_upload(cx));
                Poll::Ready(Err(answer.err().unwrap_or_else(closed)))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // A block sent is the upload's; there is nothing to flush.
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            match this.state {
                State::Open { .. } if this.written != this.size => {
                    let short = too_short(&this.path, this.written, this.size);
                    this.state = State::Closing {
                        failure: Some(short),
                    };
                }
                State::Open { .. } => {
                    // No room means the upload has stopped; its answer says why.
                    if let Some(permit) = ready!(this.poll_room(cx)) {
                        drop(permit.send(Block::End));
                    }
                    this.state = State::Closing { failure: None };
                }
                State::Closing { .. } => {
                    let answer = ready!(this.poll_upload(cx));
                    let State::Closing { failure } =
                        std::mem::replace(&mut this.state, State::Closed)
                    else {
                        return Poll::Ready(Err(closed()));
                    };
                    return Poll::Ready(match failure {
                        Some(failure) => Err(failure),
                        None => answer,
                    });
                }
                State::Closed => return Poll::Ready(Ok(())),
            }
        }
    }
}

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
