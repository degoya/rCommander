//! The `Vfs` read and write streams over one FTP transfer.
//!
//! A transfer is only good once the server confirms it (`226`), so the confirmation is part of
//! the stream: a download reads it when the data ends and fails the read that would report the
//! end, an upload reads it in `shutdown()`. Each stream holds its own leased connection; when it
//! completes, the connection goes back to the pool. A stream dropped half-way - also while it
//! waits for the confirmation - closes its connection instead: the server's answer to the
//! abandoned transfer is never read.
//!
//! A data connection on which no byte moves for the configured timeout fails with `TimedOut`
//! and is abandoned: a server that stalls a transfer cannot hold a copy forever. The clock
//! starts again with every byte read or written.

use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::{Instant, Sleep},
};

use super::{connection::Transfer, error::Failure, pool::Lease};

type Finishing = Pin<Box<dyn Future<Output = io::Result<()>> + Send>>;

enum State {
    Open { transfer: Transfer, lease: Lease },
    Finishing(Finishing),
    Done,
}

/// Closes the transfer, reads the completion reply, and lets the lease go back to the pool.
fn finish(transfer: Transfer, mut lease: Lease) -> Finishing {
    Box::pin(async move {
        lease
            .finish(transfer)
            .await
            .map_err(|failure| match failure {
                Failure::Broken(error) => error,
                Failure::Reply { code, text, .. } => io::Error::new(
                    reply_kind(code),
                    format!("the FTP transfer failed: {code} {text}"),
                ),
            })
    })
}

/// The kind of a transfer the server ended with `code`: the connection's end (`421` closing,
/// `425` no data connection, `426` transfer aborted) is `ConnectionAborted`, which the operations
/// queue retries and resumes (RC-060-07); any other refusal is `Other`.
fn reply_kind(code: u32) -> io::ErrorKind {
    match code {
        421 | 425 | 426 => io::ErrorKind::ConnectionAborted,
        _ => io::ErrorKind::Other,
    }
}

/// Drives the completion of a stream; called once the data is through.
fn poll_finishing(state: &mut State, context: &mut Context<'_>) -> Poll<io::Result<()>> {
    loop {
        match state {
            State::Open { .. } => match std::mem::replace(state, State::Done) {
                State::Open { transfer, lease } => {
                    *state = State::Finishing(finish(transfer, lease))
                }
                State::Finishing(_) | State::Done => {}
            },
            State::Finishing(future) => {
                let result = ready!(future.as_mut().poll(context));
                *state = State::Done;
                return Poll::Ready(result);
            }
            State::Done => return Poll::Ready(Ok(())),
        }
    }
}

/// The idle limit of a data connection: it expires when no byte moved for `limit`.
struct Idle {
    limit: Duration,
    timer: Pin<Box<Sleep>>,
}

impl Idle {
    fn new(limit: Duration) -> Self {
        Self {
            limit,
            timer: Box::pin(tokio::time::sleep(limit)),
        }
    }

    /// Bytes moved: the limit counts from now.
    fn progress(&mut self) {
        self.timer.as_mut().reset(Instant::now() + self.limit);
    }

    /// The error once the limit passed without progress; registers the wake-up otherwise.
    fn poll_expired(&mut self, context: &mut Context<'_>) -> Poll<io::Error> {
        ready!(self.timer.as_mut().poll(context));
        Poll::Ready(io::Error::new(
            io::ErrorKind::TimedOut,
            "the FTP data connection stalled",
        ))
    }
}

/// Discards the connection of a transfer that failed or was abandoned. One dropped while it
/// finishes needs nothing here: the lease inside the future is still owed the completion reply
/// and closes its connection when the future goes (`Lease::pending`, RC-080-40).
fn abandon(state: &mut State) {
    if let State::Open { lease, .. } = state {
        lease.discard();
    }
}

/// A file being read from the server.
pub(crate) struct Download {
    state: State,
    idle: Idle,
}

impl Download {
    pub fn new(transfer: Transfer, lease: Lease) -> Self {
        Self {
            idle: Idle::new(lease.timeout()),
            state: State::Open { transfer, lease },
        }
    }
}

impl AsyncRead for Download {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if let State::Open { transfer, .. } = &mut this.state {
            let before = buffer.filled().len();
            let read = match Pin::new(transfer).poll_read(context, buffer) {
                Poll::Ready(read) => read,
                Poll::Pending => match this.idle.poll_expired(context) {
                    Poll::Ready(stalled) => Err(stalled),
                    Poll::Pending => return Poll::Pending,
                },
            };
            if let Err(error) = read {
                abandon(&mut this.state);
                this.state = State::Done;
                return Poll::Ready(Err(error));
            }
            if buffer.filled().len() > before {
                this.idle.progress();
                return Poll::Ready(Ok(()));
            }
        }
        // The data ended: only the server's confirmation makes it the whole file.
        poll_finishing(&mut this.state, context)
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        abandon(&mut self.state);
    }
}

/// A file being written to the server. `shutdown()` completes it.
pub(crate) struct Upload {
    state: State,
    idle: Idle,
}

impl Upload {
    pub fn new(transfer: Transfer, lease: Lease) -> Self {
        Self {
            idle: Idle::new(lease.timeout()),
            state: State::Open { transfer, lease },
        }
    }

    fn closed() -> io::Error {
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            "the FTP upload is already closed",
        )
    }
}

impl AsyncWrite for Upload {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let State::Open { transfer, .. } = &mut this.state else {
            return Poll::Ready(Err(Self::closed()));
        };
        let result = match Pin::new(transfer).poll_write(context, bytes) {
            Poll::Ready(result) => result,
            Poll::Pending => match this.idle.poll_expired(context) {
                Poll::Ready(stalled) => Err(stalled),
                Poll::Pending => return Poll::Pending,
            },
        };
        match &result {
            Ok(written) if *written > 0 => this.idle.progress(),
            Ok(_) => {}
            Err(_) => {
                abandon(&mut this.state);
                this.state = State::Done;
            }
        }
        Poll::Ready(result)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.get_mut().state {
            State::Open { transfer, .. } => Pin::new(transfer).poll_flush(context),
            State::Finishing(_) | State::Done => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        poll_finishing(&mut self.get_mut().state, context)
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        abandon(&mut self.state);
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
