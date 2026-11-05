//! What is left of a local program, and the end of a closed terminal's program on Unix
//! (RC-080-62).
//!
//! [`Life`] is what the reader, the waiter and the back end know about the program's end: the
//! waiter marks it reaped, the reader marks the PTY's output ended. A terminal is over when both
//! hold: the program is reaped and nothing holds the PTY open any more.
//!
//! On Unix a closed terminal hangs its program up (`SIGHUP`, `portable-pty`'s killer). A
//! program that ignores the hang-up (`nohup`, `trap '' HUP`, a daemon) used to run on, and with
//! it the reader and the waiter thread. [`escalate`] gives it [`END_GRACE`], then sends
//! `SIGTERM`, after another [`END_GRACE`] `SIGKILL`; a program that ended on the hang-up gets
//! neither. Windows needs none of this: the job object ends the program (`job.rs`).

use crate::recover::LockOrRecover as _;
use std::{
    sync::{Condvar, Mutex, MutexGuard},
    time::Duration,
};

/// How long a closed terminal's program gets after the hang-up, and again after `SIGTERM`.
#[cfg_attr(not(unix), allow(dead_code, reason = "the escalation is Unix's"))]
pub(super) const END_GRACE: Duration = Duration::from_secs(2);

/// What the threads of a local program know about its end.
#[derive(Default)]
pub(super) struct Life {
    state: Mutex<LifeState>,
    changed: Condvar,
}

#[derive(Debug, Default, Clone, Copy)]
struct LifeState {
    /// The waiter has reaped the program: its id may name another process from now on.
    exited: bool,
    /// The reader has reached the end of the PTY's output: nothing holds the PTY open.
    drained: bool,
}

impl LifeState {
    fn is_over(self) -> bool {
        self.exited && self.drained
    }
}

impl Life {
    pub(super) fn mark_exited(&self) {
        self.lock().exited = true;
        self.changed.notify_all();
    }

    pub(super) fn mark_drained(&self) {
        self.lock().drained = true;
        self.changed.notify_all();
    }

    /// Runs `act` unless the program has been reaped, with the reap held off while it runs: a
    /// signal by the program's id never reaches another process.
    pub(super) fn unless_exited(&self, act: impl FnOnce()) {
        let state = self.lock();
        if !state.exited {
            act();
        }
    }

    /// Waits up to `timeout` for the end of the PTY's output; whether it came.
    pub(super) fn wait_drained(&self, timeout: Duration) -> bool {
        self.wait_for(timeout, |state| state.drained)
    }

    /// Waits up to `timeout` until the terminal is over; whether it is.
    #[cfg_attr(not(unix), allow(dead_code, reason = "the escalation is Unix's"))]
    pub(super) fn wait_over(&self, timeout: Duration) -> bool {
        self.wait_for(timeout, LifeState::is_over)
    }

    fn wait_for(&self, timeout: Duration, done: impl Fn(LifeState) -> bool) -> bool {
        let state = self.lock();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, timeout, |state| !done(*state))
            .unwrap_or_else(|poisoned| crate::recover::recovered(poisoned, "ending.state"));
        done(*state)
    }

    fn lock(&self) -> MutexGuard<'_, LifeState> {
        self.state.lock_or_recover("ending.state")
    }
}

/// The step after the hang-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(unix), allow(dead_code, reason = "the escalation is Unix's"))]
pub(super) enum Escalation {
    /// `SIGTERM`: a program may still tidy up.
    Terminate,
    /// `SIGKILL`: no program can ignore it.
    Kill,
}

/// After the hang-up: `send(Terminate)`, then `send(Kill)`, each only when the terminal is not
/// over `grace` after the step before.
#[cfg_attr(not(unix), allow(dead_code, reason = "the escalation is Unix's"))]
pub(super) fn escalate(life: &Life, grace: Duration, mut send: impl FnMut(Escalation)) {
    for step in [Escalation::Terminate, Escalation::Kill] {
        if life.wait_over(grace) {
            return;
        }
        send(step);
    }
}

#[cfg(test)]
#[path = "ending_tests.rs"]
mod tests;
