//! The lock state: `Absent`, `Locked`, `Unlocked`, the idle lock, the longest time unlocked and
//! the delay after failed password attempts. Pure and synchronous; [`crate::Vault`] holds one
//! behind a mutex.
//!
//! Only the person's own use defers the idle lock ([`Use::Person`]): a secret an agent's call
//! reads over MCP ([`Use::Agent`]) leaves it where it is, so an agent cannot keep the vault open
//! for ever. However often the person uses it, the vault locks at the latest
//! [`MAX_UNLOCKED_KEY`] hours after it was unlocked.

use std::{
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{Result, VaultError, kdf::VaultKey};

/// The setting that holds the idle lock in minutes; `0` never locks.
pub const IDLE_LOCK_KEY: &str = "vault.idle_lock_minutes";

/// The idle lock when the setting was never written.
pub const DEFAULT_IDLE_LOCK_MINUTES: u32 = 15;

/// The setting that holds the longest time the vault stays unlocked, in hours from the unlock,
/// use or no use; `0` sets no limit.
pub const MAX_UNLOCKED_KEY: &str = "vault.max_unlocked_hours";

/// The longest time unlocked when the setting was never written: a working day.
pub const DEFAULT_MAX_UNLOCKED_HOURS: u32 = 8;

/// Failed attempts that cost nothing; from the next one on, each waits longer.
const FREE_FAILURES: u32 = 3;

/// The longest wait before an attempt.
const MAX_FAILURE_DELAY: Duration = Duration::from_secs(30);

/// Where the vault takes the time from: the system's monotonic clock, or a test's.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// The monotonic system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// A clock that moves only when told to: for tests of the idle lock. Clones share the time.
#[derive(Debug, Clone)]
pub struct ManualClock {
    start: Instant,
    offset: Arc<Mutex<Duration>>,
}

impl ManualClock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            offset: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    /// Moves the time forward by `by`.
    pub fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap_or_else(PoisonError::into_inner) += by;
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.start + *self.offset.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Whose use of the key it is: only the person's defers the idle lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// An action in the window: storing, reading, connecting.
    Person,
    /// An agent's call over MCP: the key opens, the idle lock stays where it was.
    Agent,
}

/// The vault's state. `since` is when it was last unlocked, `last_use` the person's last
/// encryption, decryption or [`crate::Vault::touch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultState {
    /// No vault was created: no secret is stored and no password is ever asked.
    Absent,
    /// A vault exists; its key is not in memory.
    Locked,
    /// The key is in memory.
    Unlocked { since: Instant, last_use: Instant },
}

/// [`VaultState`] without the times, as the status reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultStateKind {
    Absent,
    Locked,
    Unlocked,
}

/// What `vault.status` answers: never key material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultStatus {
    pub state: VaultStateKind,
    /// The idle lock in minutes; `0` never locks.
    pub idle_lock_minutes: u32,
    /// Seconds until the vault locks - the idle lock or the longest time unlocked, whichever
    /// comes first - while unlocked and either is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locks_in_secs: Option<u64>,
}

impl VaultStatus {
    /// The status where no vault is open at all (an engine without a database).
    #[must_use]
    pub fn absent() -> Self {
        Self {
            state: VaultStateKind::Absent,
            idle_lock_minutes: DEFAULT_IDLE_LOCK_MINUTES,
            locks_in_secs: None,
        }
    }
}

/// The state with the key it implies: a key exactly while `Unlocked`.
pub(crate) struct Machine {
    state: VaultState,
    key: Option<VaultKey>,
    idle_lock_minutes: u32,
    max_unlocked_hours: u32,
    failures: u32,
}

impl Machine {
    pub(crate) fn new(exists: bool, idle_lock_minutes: u32) -> Self {
        Self {
            state: if exists {
                VaultState::Locked
            } else {
                VaultState::Absent
            },
            key: None,
            idle_lock_minutes,
            max_unlocked_hours: DEFAULT_MAX_UNLOCKED_HOURS,
            failures: 0,
        }
    }

    pub(crate) fn state(&self) -> VaultState {
        self.state
    }

    pub(crate) fn idle_lock_minutes(&self) -> u32 {
        self.idle_lock_minutes
    }

    pub(crate) fn set_idle_lock_minutes(&mut self, minutes: u32) {
        self.idle_lock_minutes = minutes;
    }

    pub(crate) fn set_max_unlocked_hours(&mut self, hours: u32) {
        self.max_unlocked_hours = hours;
    }

    fn idle_limit(&self) -> Option<Duration> {
        (self.idle_lock_minutes > 0)
            .then(|| Duration::from_secs(60 * u64::from(self.idle_lock_minutes)))
    }

    fn unlocked_limit(&self) -> Option<Duration> {
        (self.max_unlocked_hours > 0)
            .then(|| Duration::from_secs(3600 * u64::from(self.max_unlocked_hours)))
    }

    /// When the vault locks, while unlocked: the idle lock or the longest time unlocked,
    /// whichever comes first; `None` when neither is set.
    pub(crate) fn lock_deadline(&self) -> Option<Instant> {
        let VaultState::Unlocked { since, last_use } = self.state else {
            return None;
        };
        let idle = self.idle_limit().map(|limit| last_use + limit);
        let longest = self.unlocked_limit().map(|limit| since + limit);
        idle.into_iter().chain(longest).min()
    }

    pub(crate) fn status(&self, now: Instant) -> VaultStatus {
        let state = match self.state {
            VaultState::Absent => VaultStateKind::Absent,
            VaultState::Locked => VaultStateKind::Locked,
            VaultState::Unlocked { .. } => VaultStateKind::Unlocked,
        };
        VaultStatus {
            state,
            idle_lock_minutes: self.idle_lock_minutes,
            locks_in_secs: self
                .lock_deadline()
                .map(|deadline| deadline.saturating_duration_since(now).as_secs()),
        }
    }

    /// The vault now exists and is unlocked with `key`; the failure count and the longest time
    /// unlocked start over - the person has just typed the master password.
    pub(crate) fn unlocked(&mut self, key: VaultKey, now: Instant) {
        // The old key, if any, drops (and is zeroized) here.
        self.key = Some(key);
        self.state = VaultState::Unlocked {
            since: now,
            last_use: now,
        };
        self.failures = 0;
    }

    /// A vault exists (another process created it); locked unless it already was unlocked.
    pub(crate) fn exists(&mut self) {
        if self.state == VaultState::Absent {
            self.state = VaultState::Locked;
        }
    }

    /// The vault was removed: no key, no failures, `Absent`.
    pub(crate) fn removed(&mut self) {
        self.key = None;
        self.state = VaultState::Absent;
        self.failures = 0;
    }

    /// Drops the key; says whether the vault was unlocked.
    pub(crate) fn lock(&mut self) -> bool {
        let was_unlocked = matches!(self.state, VaultState::Unlocked { .. });
        self.key = None;
        if was_unlocked {
            self.state = VaultState::Locked;
        }
        was_unlocked
    }

    /// Locks when the idle lock or the longest time unlocked has fallen due at `now`; says
    /// whether it locked.
    pub(crate) fn lock_if_idle(&mut self, now: Instant) -> bool {
        match self.lock_deadline() {
            Some(deadline) if now >= deadline => self.lock(),
            _ => false,
        }
    }

    /// The key for one use at `now`; the person's use defers the idle lock, an agent's does
    /// not. A lock that fell due locks first, so a late timer never lets a stale key through.
    pub(crate) fn key_for_use(&mut self, now: Instant, by: Use) -> Result<&VaultKey> {
        self.lock_if_idle(now);
        match (&mut self.state, &self.key) {
            (VaultState::Unlocked { last_use, .. }, Some(key)) => {
                if by == Use::Person {
                    *last_use = now;
                }
                Ok(key)
            }
            (VaultState::Absent, _) => Err(VaultError::Absent),
            _ => Err(VaultError::Locked),
        }
    }

    /// One more failed attempt; answers how long the next one waits.
    pub(crate) fn failed(&mut self, base: Duration) -> Duration {
        self.failures = self.failures.saturating_add(1);
        failure_delay(self.failures, base)
    }

    /// How long the next attempt waits.
    pub(crate) fn pending_delay(&self, base: Duration) -> Duration {
        failure_delay(self.failures, base)
    }
}

/// The wait before an attempt after `failures` failed ones: nothing for the first
/// [`FREE_FAILURES`], then `base`, doubling, at most [`MAX_FAILURE_DELAY`].
pub(crate) fn failure_delay(failures: u32, base: Duration) -> Duration {
    let Some(over) = failures.checked_sub(FREE_FAILURES) else {
        return Duration::ZERO;
    };
    base.checked_mul(1_u32 << over.min(16))
        .unwrap_or(MAX_FAILURE_DELAY)
        .min(MAX_FAILURE_DELAY)
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
