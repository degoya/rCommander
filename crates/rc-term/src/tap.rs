//! A copy of a back end's output for whoever talks to the program before the person does: the
//! login script of an SSH terminal (RC-060-03) waits for prompts in it and types through it.
//!
//! [`tap`] puts itself between a started back end and its session. The session still gets every
//! event in order; the [`Tap`] gets a copy of each output chunk until it is dropped or the
//! program ends, and writes to the program directly, past the session's held input
//! ([`crate::TermSession::hold_input`]). A tap that falls [`TAP_QUEUE`] chunks behind misses the
//! next ones rather than holding up the screen.

use std::sync::Arc;

use tokio::sync::mpsc::{self, error::TrySendError};

use crate::{BackendEvent, Spawned, TermBackend, TermError, TermSize};

/// Output chunks a tap may fall behind before it misses some.
pub const TAP_QUEUE: usize = 256;

/// Events in flight between the tap's task and the session.
const EVENT_QUEUE: usize = 64;

/// The program's output, copied, and its input, for one reader.
pub struct Tap {
    output: mpsc::Receiver<Vec<u8>>,
    backend: Arc<dyn TermBackend>,
}

impl Tap {
    /// The next chunk of output; `None` once the program ended.
    pub async fn read(&mut self) -> Option<Vec<u8>> {
        self.output.recv().await
    }

    /// Sends `bytes` to the program, held input or not.
    ///
    /// # Errors
    ///
    /// [`TermError::Exited`] once the program has ended.
    pub fn write(&self, bytes: Vec<u8>) -> Result<(), TermError> {
        self.backend.write(bytes)
    }
}

impl std::fmt::Debug for Tap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Tap").finish_non_exhaustive()
    }
}

/// `spawned` with a [`Tap`] on its output. Runs a task on the current tokio runtime.
///
/// # Panics
///
/// Outside a tokio runtime.
#[must_use]
pub fn tap(spawned: Spawned) -> (Spawned, Tap) {
    let backend: Arc<dyn TermBackend> = Arc::from(spawned.backend);
    let (copies, output) = mpsc::channel(TAP_QUEUE);
    let (events, received) = mpsc::channel(EVENT_QUEUE);
    let mut source = spawned.events;
    tokio::spawn(async move {
        let mut copies = Some(copies);
        while let Some(event) = source.recv().await {
            match &event {
                BackendEvent::Output(bytes) => {
                    let sent = copies.as_ref().map(|sender| sender.try_send(bytes.clone()));
                    match sent {
                        Some(Err(TrySendError::Full(_))) => {
                            tracing::debug!("a terminal tap fell behind and missed output");
                        }
                        Some(Err(TrySendError::Closed(_))) => copies = None,
                        Some(Ok(())) | None => {}
                    }
                }
                // The tap reads its end before the session hears of it.
                BackendEvent::Exited(_) => copies = None,
            }
            if events.send(event).await.is_err() {
                break;
            }
        }
    });
    let shared = Spawned {
        backend: Box::new(Shared(Arc::clone(&backend))),
        events: received,
    };
    (shared, Tap { output, backend })
}

/// The back end, shared by the session and the tap.
struct Shared(Arc<dyn TermBackend>);

impl TermBackend for Shared {
    fn write(&self, bytes: Vec<u8>) -> Result<(), TermError> {
        self.0.write(bytes)
    }

    fn resize(&self, size: TermSize) -> Result<(), TermError> {
        self.0.resize(size)
    }

    fn shutdown(&self) {
        self.0.shutdown();
    }

    fn process_id(&self) -> Option<u32> {
        self.0.process_id()
    }
}

#[cfg(test)]
#[path = "tap_tests.rs"]
mod tests;
