//! How many calls of one plugin run at once: its manifest's `limits.max_concurrent` (RC-080-43).
//! A call beyond it waits until one ends. Every running call is bounded by its wall clock, so
//! the wait is bounded as well, and a caller that gives up (Quick View moved on) drops its wait.

use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::Notify;

/// The calls of one plugin that run, shared by all its stores and versions.
#[derive(Debug, Default)]
pub(crate) struct CallGate {
    running: Mutex<u32>,
    ended: Notify,
}

impl CallGate {
    /// Waits until fewer than `max` (at least one) calls run, and counts this one in.
    pub(crate) async fn enter(self: &Arc<Self>, max: u32) -> CallSlot {
        let max = max.max(1);
        loop {
            let mut ended = std::pin::pin!(self.ended.notified());
            // Registered before the count is read, so an end in between is not missed.
            ended.as_mut().enable();
            {
                let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
                if *running < max {
                    *running += 1;
                    return CallSlot {
                        gate: Arc::clone(self),
                    };
                }
            }
            ended.await;
        }
    }

    /// How many calls run now.
    #[cfg(test)]
    pub(crate) fn running(&self) -> u32 {
        *self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One running call; dropped, it lets a waiting call in.
#[derive(Debug)]
#[must_use = "the call counts as running only while its slot is held"]
pub struct CallSlot {
    gate: Arc<CallGate>,
}

impl Drop for CallSlot {
    fn drop(&mut self) {
        {
            let mut running = self
                .gate
                .running
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *running = running.saturating_sub(1);
        }
        self.gate.ended.notify_waiters();
    }
}

#[cfg(test)]
#[path = "call_gate_tests.rs"]
mod tests;
