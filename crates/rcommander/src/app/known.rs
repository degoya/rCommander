//! A value the window knows for the run (RC-110-30): what the engine answered once - the
//! workspaces, the own commands, the Start menu, the internal associations - kept in a static
//! behind a read-write lock, so whatever draws reads it without asking the engine. One holder
//! for the three `known.rs` (`workspaces`, `user_commands`, `associations`), which keep their
//! own functions around it.

use std::sync::RwLock;

use crate::recover::RwLockOrRecover as _;

/// A value set from the engine's answers and read by the window; `name` is the lock's name in
/// the log, should a panic poison it.
pub struct Known<T> {
    name: &'static str,
    value: RwLock<T>,
}

impl<T> Known<T> {
    /// A holder of `value` until the first [`Known::set`].
    pub const fn new(name: &'static str, value: T) -> Self {
        Self {
            name,
            value: RwLock::new(value),
        }
    }

    /// The value from now on.
    #[track_caller]
    pub fn set(&self, value: T) {
        *self.value.write_or_recover(self.name) = value;
    }

    /// `look` at the value without a copy of it. `look` runs under the lock, so it reads and
    /// returns - it neither sets the value nor asks for it again.
    #[track_caller]
    pub fn with<R>(&self, look: impl FnOnce(&T) -> R) -> R {
        look(&self.value.read_or_recover(self.name))
    }

    /// A copy of the value.
    #[track_caller]
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.read_or_recover(self.name).clone()
    }
}

#[cfg(test)]
#[path = "known_tests.rs"]
mod tests;
