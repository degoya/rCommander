//! Pause, resume and cancel: flags a running job looks at between two buffers. A job of a
//! stopped list (RC-010-15) is held besides: it waits for its start without being paused.

use std::{future::Future, sync::Arc};

use tokio::sync::watch;

/// The caller's side of a job's flags. Clones share them.
#[derive(Debug, Clone)]
pub(crate) struct Control {
    flags: Arc<watch::Sender<Flags>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Flags {
    paused: bool,
    cancelled: bool,
    /// Its list is stopped: the job does not start yet.
    held: bool,
}

/// The job is to stop: the caller cancelled it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cancelled;

impl Default for Control {
    fn default() -> Self {
        Self {
            flags: Arc::new(watch::Sender::new(Flags::default())),
        }
    }
}

impl Control {
    /// Pauses; whether that changed anything.
    pub(crate) fn pause(&self) -> bool {
        self.flags.send_if_modified(|flags| {
            let changed = !flags.paused && !flags.cancelled;
            flags.paused = true;
            changed
        })
    }

    /// Resumes; whether that changed anything.
    pub(crate) fn resume(&self) -> bool {
        self.flags.send_if_modified(|flags| {
            let changed = flags.paused;
            flags.paused = false;
            changed
        })
    }

    /// Cancels, also a paused job.
    pub(crate) fn cancel(&self) {
        self.flags.send_modify(|flags| flags.cancelled = true);
    }

    /// Holds the job back from starting, or lets it start; whether that changed anything.
    pub(crate) fn hold(&self, held: bool) -> bool {
        self.flags.send_if_modified(|flags| {
            let changed = flags.held != held;
            flags.held = held;
            changed
        })
    }

    pub(crate) fn is_held(&self) -> bool {
        self.flags.borrow().held
    }

    /// Resolves once the job is no longer held; fails once cancelled.
    pub(crate) async fn released(&self) -> Result<(), Cancelled> {
        let mut flags = self.flags.subscribe();
        let flags = flags
            .wait_for(|flags| flags.cancelled || !flags.held)
            .await
            .map_err(|_| Cancelled)?;
        if flags.cancelled {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.flags.borrow().paused
    }

    /// Whether the job was cancelled, for work that runs off the async threads (packing).
    pub(crate) fn is_cancelled(&self) -> bool {
        self.flags.borrow().cancelled
    }

    /// Returns at once while running, waits while paused, and fails once cancelled.
    pub(crate) async fn checkpoint(&self) -> Result<(), Cancelled> {
        let mut flags = self.flags.subscribe();
        let flags = flags
            .wait_for(|flags| flags.cancelled || !flags.paused)
            .await
            .map_err(|_| Cancelled)?;
        if flags.cancelled {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }

    /// Resolves when the job is cancelled.
    pub(crate) async fn cancelled(&self) {
        let mut flags = self.flags.subscribe();
        let _ = flags.wait_for(|flags| flags.cancelled).await;
    }

    /// `work` - one read, one write, a wait - unless the job is cancelled first: the cancel is
    /// looked at before `work` each time (`biased`), and `work` is dropped then. The one place
    /// the queue's loops race their work against a cancel (RC-080-48).
    pub(crate) async fn or_cancel<T>(&self, work: impl Future<Output = T>) -> Result<T, Cancelled> {
        tokio::select! {
            biased;
            () = self.cancelled() => Err(Cancelled),
            done = work => Ok(done),
        }
    }
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
