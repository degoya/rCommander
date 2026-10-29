//! What each plugin call did: which plugin, which version, which export, how it ended, how long.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/diagnostics.rs`
//! (`ExecutionLog`, `ExecutionOutcome`) at commit 26c8592a6. Kept in memory here, newest last and
//! bounded, and handed to a [`ExecutionSink`] as each call ends: the engine's writes them into the
//! table `plugin_executions` in `RETAINED_TABLES` (RC-100-09). A record holds codes, never a message, so nothing a plugin wrote reaches it.
//! New beside rDownloader's classes: `Cancelled` (the person moved on) and `Refused` (the host
//! refused the plugin's output).

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};

use rc_plugin_types::Failure;

use crate::failure::codes;

/// Records kept in memory, over every plugin together.
pub const MAX_EXECUTION_RECORDS: usize = 1000;

/// How one call ended, in terms a person can act on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionOutcome {
    /// The call returned a result.
    Ok,
    /// The plugin reported a failure of its own: a broken file, an unsupported variant.
    Failed,
    /// A trap: an `unreachable`, a panic.
    Crash,
    /// It ran out of wall-clock budget.
    Timeout,
    /// It ran out of instruction budget.
    Fuel,
    /// It asked for more memory than its manifest allows.
    Memory,
    /// It reached for something its manifest does not grant.
    Denied,
    /// The caller stopped it.
    Cancelled,
    /// It answered something the host refused.
    Refused,
    /// The host could not serve it: a source or sink failed under it.
    HostError,
}

impl ExecutionOutcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Crash => "crash",
            Self::Timeout => "timeout",
            Self::Fuel => "fuel",
            Self::Memory => "memory",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::Refused => "refused",
            Self::HostError => "host_error",
        }
    }

    /// Whether the outcome is the plugin malfunctioning, which counts towards suspension
    /// (`docs/plugins.md`, "Process model").
    #[must_use]
    pub fn counts_as_failure(self) -> bool {
        matches!(
            self,
            Self::Crash | Self::Timeout | Self::Fuel | Self::Memory | Self::Denied
        )
    }

    /// Classifies a result by the code the host attached to it.
    #[must_use]
    pub fn of<T>(result: &Result<T, Failure>) -> Self {
        let Err(failure) = result else {
            return Self::Ok;
        };
        match failure.code() {
            Some(codes::PLUGIN_CRASHED) => Self::Crash,
            Some(codes::PLUGIN_TIMED_OUT) => Self::Timeout,
            Some(codes::PLUGIN_OUT_OF_FUEL) => Self::Fuel,
            Some(codes::PLUGIN_OUT_OF_MEMORY) => Self::Memory,
            Some(codes::PLUGIN_DENIED) => Self::Denied,
            Some(codes::PLUGIN_CANCELLED) => Self::Cancelled,
            Some(
                codes::PLUGIN_BAD_OUTPUT
                | codes::PLUGIN_OUTPUT_TOO_LARGE
                | codes::PLUGIN_WRITE_TOO_LARGE
                | codes::PLUGIN_UNKNOWN_HANDLE,
            ) => Self::Refused,
            Some(
                codes::PLUGIN_SINK_FAILED | codes::PLUGIN_SOURCE_FAILED | codes::PLUGIN_HOST_FAILED,
            ) => Self::HostError,
            _ => Self::Failed,
        }
    }
}

/// One finished call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionRecord {
    pub plugin_id: String,
    pub plugin_version: String,
    /// The plugin kind, as the manifest names it (`viewer`).
    pub plugin_type: &'static str,
    /// The export called (`open`, `text`, ...).
    pub operation: &'static str,
    pub outcome: ExecutionOutcome,
    /// The failure's code, when it failed.
    pub code: Option<String>,
    pub started_at: SystemTime,
    pub duration: Duration,
}

/// Who a call belongs to, for its record.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PluginIdentity {
    pub id: String,
    pub name: String,
    pub version: String,
    pub plugin_type: &'static str,
}

/// One call in flight.
#[derive(Debug)]
pub struct Invocation {
    operation: &'static str,
    started: Instant,
    started_at: SystemTime,
}

/// Where each finished call's record goes besides memory. Called on the plugin call's thread, so
/// it only hands the record on (a channel), never waits for a disk.
pub type ExecutionSink = Arc<dyn Fn(&ExecutionRecord) + Send + Sync>;

/// The records of the calls, bounded, newest last.
#[derive(Default)]
pub struct ExecutionLog {
    records: Mutex<VecDeque<ExecutionRecord>>,
    sink: OnceLock<ExecutionSink>,
}

impl std::fmt::Debug for ExecutionLog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExecutionLog")
            .field("records", &self.records)
            .field("sink", &self.sink.get().is_some())
            .finish()
    }
}

impl ExecutionLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Hands every record from now on to `sink` as well; `false` when one was set already (the
    /// first one stays).
    pub fn set_sink(&self, sink: ExecutionSink) -> bool {
        self.sink.set(sink).is_ok()
    }

    /// Starts timing one call of `operation`.
    #[must_use]
    pub fn begin(&self, operation: &'static str) -> Invocation {
        Invocation {
            operation,
            started: Instant::now(),
            started_at: SystemTime::now(),
        }
    }

    /// Files a finished call and answers how it ended. Never fails the caller.
    pub fn finish<T>(
        &self,
        identity: &PluginIdentity,
        invocation: Invocation,
        result: &Result<T, Failure>,
    ) -> ExecutionOutcome {
        let outcome = ExecutionOutcome::of(result);
        let code = result
            .as_ref()
            .err()
            .and_then(|failure| failure.code().map(str::to_owned));
        if outcome != ExecutionOutcome::Ok {
            tracing::debug!(
                plugin_id = identity.id.as_str(),
                version = identity.version.as_str(),
                operation = invocation.operation,
                outcome = outcome.as_str(),
                "plugin call ended"
            );
        }
        let record = ExecutionRecord {
            plugin_id: identity.id.clone(),
            plugin_version: identity.version.clone(),
            plugin_type: identity.plugin_type,
            operation: invocation.operation,
            outcome,
            code,
            started_at: invocation.started_at,
            duration: invocation.started.elapsed(),
        };
        if let Some(sink) = self.sink.get() {
            sink(&record);
        }
        if let Ok(mut records) = self.records.lock() {
            if records.len() >= MAX_EXECUTION_RECORDS {
                records.pop_front();
            }
            records.push_back(record);
        }
        outcome
    }

    /// The records of one plugin, oldest first.
    #[must_use]
    pub fn records(&self, plugin_id: &str) -> Vec<ExecutionRecord> {
        self.records
            .lock()
            .map(|records| {
                records
                    .iter()
                    .filter(|record| record.plugin_id == plugin_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
