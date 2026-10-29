//! The SSH back end (RC-060-03): a session channel of an SSH connection with a pseudo-terminal
//! and a shell on it (`rc_remote::ssh::start_pty_shell` asked for both).
//!
//! Two tasks per channel. The reader turns the channel's messages into [`BackendEvent`]s: data
//! (and stderr, which a PTY merges anyway) as output, the exit status kept for the end, the
//! channel's close - or the connection's end - as [`BackendEvent::Exited`]. The writer sends
//! what [`TermBackend`] queued: input as data, a size as `window-change`, the end as EOF and
//! close. A server that never answers the close is given [`CLOSE_GRACE`]; the reader then ends
//! by itself. Whatever keeps the connection counted as in use (the engine's channel lease) is
//! held by the reader and dropped when the channel is done.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use russh::{Channel, ChannelMsg, ChannelReadHalf, ChannelWriteHalf, client::Msg};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};

use crate::{BackendEvent, Spawned, TermBackend, TermError, TermSize};

/// A session channel of an SSH client connection.
pub type SshTermChannel = Channel<Msg>;

/// Output chunks in flight between the reader and the session before the reader waits.
const OUTPUT_QUEUE: usize = 64;

/// How long the reader waits for the server's close after the terminal asked for the end.
const CLOSE_GRACE: Duration = Duration::from_secs(3);

/// What the writer sends.
enum Input {
    Bytes(Vec<u8>),
    Resize(TermSize),
    Close,
}

struct SshBackend {
    input: mpsc::UnboundedSender<Input>,
    ended: Arc<AtomicBool>,
}

impl SshBackend {
    fn send(&self, input: Input) -> Result<(), TermError> {
        if self.ended.load(Ordering::SeqCst) {
            return Err(TermError::Exited);
        }
        self.input.send(input).map_err(|_| TermError::Exited)
    }
}

impl TermBackend for SshBackend {
    fn write(&self, bytes: Vec<u8>) -> Result<(), TermError> {
        self.send(Input::Bytes(bytes))
    }

    fn resize(&self, size: TermSize) -> Result<(), TermError> {
        self.send(Input::Resize(size))
    }

    fn shutdown(&self) {
        // A writer that is gone has closed the channel already.
        let _ = self.input.send(Input::Close);
    }
}

/// Drives `channel`, which has a PTY and a shell, as a terminal back end; `keep` lives until the
/// channel is done. Runs its tasks on the current tokio runtime.
///
/// # Panics
///
/// Outside a tokio runtime.
#[must_use]
pub fn spawn_ssh<K: Send + 'static>(channel: SshTermChannel, keep: K) -> Spawned {
    let (reader, writer) = channel.split();
    let (events, received) = mpsc::channel(OUTPUT_QUEUE);
    let (input, queued) = mpsc::unbounded_channel();
    let (closing, closing_seen) = watch::channel(false);
    let ended = Arc::new(AtomicBool::new(false));
    tokio::spawn(write_loop(writer, queued, closing));
    tokio::spawn(read_loop(
        reader,
        events,
        closing_seen,
        Arc::clone(&ended),
        keep,
    ));
    Spawned {
        backend: Box::new(SshBackend { input, ended }),
        events: received,
    }
}

async fn write_loop(
    writer: ChannelWriteHalf<Msg>,
    mut queued: mpsc::UnboundedReceiver<Input>,
    closing: watch::Sender<bool>,
) {
    while let Some(input) = queued.recv().await {
        let sent = match input {
            Input::Bytes(bytes) => writer.data_bytes(bytes).await,
            Input::Resize(size) => {
                writer
                    .window_change(u32::from(size.cols), u32::from(size.rows), 0, 0)
                    .await
            }
            Input::Close => break,
        };
        if let Err(error) = sent {
            tracing::debug!(%error, "the SSH terminal's channel takes no more input");
            break;
        }
    }
    // Asked to end, every handle dropped, or the channel failed: the end in each case. Both
    // fail harmlessly on a channel the server closed already.
    let _ = writer.eof().await;
    let _ = writer.close().await;
    closing.send_replace(true);
}

async fn read_loop<K>(
    mut reader: ChannelReadHalf,
    events: mpsc::Sender<BackendEvent>,
    mut closing: watch::Receiver<bool>,
    ended: Arc<AtomicBool>,
    keep: K,
) {
    let mut code = None;
    let mut deadline: Option<Instant> = None;
    loop {
        let message = tokio::select! {
            message = reader.wait() => message,
            // An error is a writer that is gone, which is the same.
            _ = closing.wait_for(|closing| *closing), if deadline.is_none() => {
                deadline = Some(Instant::now() + CLOSE_GRACE);
                continue;
            }
            () = until(deadline) => {
                tracing::debug!("the SSH server did not close the terminal's channel in time");
                break;
            }
        };
        match message {
            Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                if events
                    .send(BackendEvent::Output(data.to_vec()))
                    .await
                    .is_err()
                {
                    // The session is gone; nobody reads the rest.
                    break;
                }
            }
            Some(ChannelMsg::ExitStatus { exit_status }) => code = Some(exit_status),
            Some(ChannelMsg::Close) | None => break,
            Some(_) => {}
        }
    }
    ended.store(true, Ordering::SeqCst);
    drop(keep);
    let _ = events.send(BackendEvent::Exited(code)).await;
}

/// Sleeps until `deadline`, or forever without one.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;
