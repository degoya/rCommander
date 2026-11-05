//! What a panic leaves behind (RC-080-71): a background task that died of one, and a lock it
//! poisoned. The panic hook writes the panic's message; [`LogPanic`] adds, where the task is
//! joined, which task it was - as `error` with the target `panic` - and [`LockOrRecover`] names
//! the lock as `warn`; then the work goes on as before. Every crate keeps its own copy of this
//! module (the owner's decision, 2026-11-05: one helper per crate).

use std::sync::{Mutex, MutexGuard};

/// A joined task's result, logged when the task died of a panic.
pub(crate) trait LogPanic: Sized {
    /// `self` unchanged; logged first, with `task` and the caller's place, when the task
    /// panicked. A cancelled task is no panic and is not logged.
    #[must_use]
    fn log_panic(self, task: &'static str) -> Self;
}

impl<T> LogPanic for Result<T, tokio::task::JoinError> {
    #[track_caller]
    fn log_panic(self, task: &'static str) -> Self {
        if let Err(error) = &self
            && error.is_panic()
        {
            panicked(task);
        }
        self
    }
}

impl<T> LogPanic for std::thread::Result<T> {
    #[track_caller]
    fn log_panic(self, task: &'static str) -> Self {
        if self.is_err() {
            panicked(task);
        }
        self
    }
}

/// The error line of a task that died of a panic.
#[track_caller]
fn panicked(task: &'static str) {
    let at = std::panic::Location::caller();
    tracing::error!(
        target: "panic",
        code = "task_panicked",
        task,
        %at,
        "a background task died of a panic"
    );
}

/// A mutex taken even when a panic poisoned it: logged once as `warn` with `name` and the
/// caller's place, its poison cleared, and its state used as it is - as before RC-080-71, only no
/// longer silently.
pub(crate) trait LockOrRecover<T: ?Sized> {
    fn lock_or_recover(&self, name: &'static str) -> MutexGuard<'_, T>;
}

impl<T: ?Sized> LockOrRecover<T> for Mutex<T> {
    #[track_caller]
    fn lock_or_recover(&self, name: &'static str) -> MutexGuard<'_, T> {
        match self.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                poisoned_lock(name);
                self.clear_poison();
                poisoned.into_inner()
            }
        }
    }
}

/// The warning for a lock a panic poisoned.
#[track_caller]
fn poisoned_lock(name: &'static str) {
    let at = std::panic::Location::caller();
    tracing::warn!(
        code = "lock_poisoned",
        lock = name,
        %at,
        "a lock a panic poisoned is taken with the state it left"
    );
}

#[cfg(test)]
#[path = "recover_tests.rs"]
mod tests;
