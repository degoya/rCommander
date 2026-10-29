//! [`IoCancel`]: the calls of a [`crate::TimeoutFs`] that wait now, and the switch that stops
//! them.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::sync::{Notify, futures::Notified};

/// Stops the calls of the [`crate::TimeoutFs`] it was given to that wait at that moment; the
/// calls after it run as usual. Clones share one switch.
#[derive(Debug, Clone, Default)]
pub struct IoCancel {
    inner: Arc<Watch>,
}

#[derive(Debug, Default)]
struct Watch {
    stop: Notify,
    /// When each waiting call started, by its number.
    waiting: Mutex<BTreeMap<u64, Instant>>,
    next: AtomicU64,
}

impl IoCancel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ends every call waiting now with `cancelled`; how many there were.
    pub fn cancel(&self) -> usize {
        let count = self.waiting().len();
        self.inner.stop.notify_waiters();
        count
    }

    /// Whether a call has waited `at_least` so long: what a window asks before it lets Escape
    /// stop a drive that does not answer.
    #[must_use]
    pub fn stalled(&self, at_least: Duration) -> bool {
        self.waiting()
            .values()
            .any(|started| started.elapsed() >= at_least)
    }

    /// A call starts waiting: its stop signal, armed at once, and the record of its wait, which
    /// ends when the guard is dropped.
    pub(crate) fn enter(&self) -> (Notified<'_>, Waiting<'_>) {
        let stop = self.inner.stop.notified();
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed);
        self.waiting().insert(id, Instant::now());
        (stop, Waiting { cancel: self, id })
    }

    fn waiting(&self) -> MutexGuard<'_, BTreeMap<u64, Instant>> {
        self.inner
            .waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// One waiting call; dropped when it ends, answered, timed out, cancelled or abandoned.
pub(crate) struct Waiting<'a> {
    cancel: &'a IoCancel,
    id: u64,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.cancel.waiting().remove(&self.id);
    }
}
