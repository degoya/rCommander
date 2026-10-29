//! A plugin that keeps failing is suspended until the person resumes it.
//!
//! New in rCommander (`docs/plugins.md`, "Process model"): rDownloader only records failures, and
//! a broken viewer would otherwise fail on every cursor move in Quick View. After
//! `plugins.crash_limit` failures - a trap, a timeout, exhausted fuel or memory, a refused import
//! ([`crate::ExecutionOutcome::counts_as_failure`]) - within [`FAILURE_WINDOW`] no call of the
//! plugin runs. The state lives in memory; the setting and the Resume button are RC-100-09's.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rc_plugin_types::{Failure, FailureKind};

use crate::{
    call_gate::{CallGate, CallSlot},
    failure::codes,
};

/// Failures counted towards a suspension are those of the last five minutes.
pub const FAILURE_WINDOW: Duration = Duration::from_secs(5 * 60);
/// `plugins.crash_limit`'s default and range.
pub const DEFAULT_CRASH_LIMIT: u32 = 3;
pub const MIN_CRASH_LIMIT: u32 = 1;
pub const MAX_CRASH_LIMIT: u32 = 10;

#[derive(Debug)]
struct State {
    limit: u32,
    failures: VecDeque<Instant>,
    suspended: bool,
}

/// The failure count and the suspension of one plugin, shared by all its stores, and how many
/// of its calls run at once.
#[derive(Debug)]
pub struct PluginHealth {
    state: Mutex<State>,
    calls: Arc<CallGate>,
}

impl Default for PluginHealth {
    fn default() -> Self {
        Self::new(DEFAULT_CRASH_LIMIT)
    }
}

impl PluginHealth {
    /// A plugin suspended after `crash_limit` failures, held to [`MIN_CRASH_LIMIT`] to
    /// [`MAX_CRASH_LIMIT`].
    #[must_use]
    pub fn new(crash_limit: u32) -> Self {
        Self {
            state: Mutex::new(State {
                limit: crash_limit.clamp(MIN_CRASH_LIMIT, MAX_CRASH_LIMIT),
                failures: VecDeque::new(),
                suspended: false,
            }),
            calls: Arc::new(CallGate::default()),
        }
    }

    /// Follows a changed `plugins.crash_limit`.
    pub fn set_crash_limit(&self, crash_limit: u32) {
        if let Ok(mut state) = self.state.lock() {
            state.limit = crash_limit.clamp(MIN_CRASH_LIMIT, MAX_CRASH_LIMIT);
        }
    }

    /// Counts one failure; `true` when it suspended the plugin.
    pub fn record_failure(&self) -> bool {
        self.record_failure_at(Instant::now())
    }

    pub(crate) fn record_failure_at(&self, now: Instant) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        while state
            .failures
            .front()
            .is_some_and(|first| now.saturating_duration_since(*first) > FAILURE_WINDOW)
        {
            state.failures.pop_front();
        }
        state.failures.push_back(now);
        if !state.suspended && state.failures.len() >= state.limit as usize {
            state.suspended = true;
            return true;
        }
        false
    }

    #[must_use]
    pub fn is_suspended(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.suspended)
            .unwrap_or(true)
    }

    /// The person's Resume: the plugin runs again, its count starts from zero.
    pub fn resume(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.suspended = false;
            state.failures.clear();
        }
    }

    /// Waits until fewer than `max_concurrent` calls of the plugin run - its manifest's
    /// `limits.max_concurrent` - and counts the caller's in until the slot is dropped.
    pub async fn enter(&self, max_concurrent: u32) -> CallSlot {
        self.calls.enter(max_concurrent).await
    }

    /// `plugin_suspended` while the plugin may not run.
    pub fn check(&self) -> Result<(), Failure> {
        if self.is_suspended() {
            return Err(Failure::coded(FailureKind::Other, codes::PLUGIN_SUSPENDED));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
