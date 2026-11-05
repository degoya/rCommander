//! What a panic leaves behind (RC-080-71): a lock it poisoned. The panic hook writes the panic's
//! message; [`LockOrRecover`] names the lock as `warn`, and the work goes on as before. Every
//! crate keeps its own copy of this module (the owner's decision, 2026-11-05: one helper per
//! crate).

use std::sync::{Mutex, MutexGuard};

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

/// What a wait on a condition variable got back when a panic poisoned its lock: logged as
/// [`LockOrRecover`] does, the state used as it is.
#[track_caller]
pub(crate) fn recovered<G>(poisoned: std::sync::PoisonError<G>, name: &'static str) -> G {
    poisoned_lock(name);
    poisoned.into_inner()
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
