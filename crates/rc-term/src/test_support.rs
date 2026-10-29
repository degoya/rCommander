//! What the crate's unit tests share (RC-080-32): a back end that records what a session sends,
//! with the program behind it as a channel the test feeds.

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::{BackendEvent, Spawned, TermBackend, TermError, TermSize};

/// A back end that records what the session sends: the bytes written, every size set, and
/// whether it was shut down. The test plays the program through the channel of [`spawned`].
#[derive(Default)]
pub(crate) struct FakeBackend {
    pub(crate) written: Mutex<Vec<u8>>,
    pub(crate) sizes: Mutex<Vec<TermSize>>,
    pub(crate) shut_down: Mutex<bool>,
}

impl TermBackend for Arc<FakeBackend> {
    fn write(&self, bytes: Vec<u8>) -> Result<(), TermError> {
        self.written.lock().expect("lock").extend(bytes);
        Ok(())
    }

    fn resize(&self, size: TermSize) -> Result<(), TermError> {
        self.sizes.lock().expect("lock").push(size);
        Ok(())
    }

    fn shutdown(&self) {
        *self.shut_down.lock().expect("lock") = true;
    }
}

/// A [`Spawned`] over a fresh [`FakeBackend`]: the fake to read from, and the program's end of
/// the event channel.
pub(crate) fn spawned() -> (Spawned, Arc<FakeBackend>, mpsc::Sender<BackendEvent>) {
    let fake = Arc::new(FakeBackend::default());
    let (program, events) = mpsc::channel(16);
    let spawned = Spawned {
        backend: Box::new(Arc::clone(&fake)),
        events,
    };
    (spawned, fake, program)
}
