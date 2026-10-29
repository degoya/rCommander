//! The host functions of `rcommander:plugin`: `host`, `source`, `sink` and `settings`,
//! implemented once on the store state; `link.rs` links them under each contract version.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/component/host_impls.rs`
//! (`log` with its budget, `now-unix-seconds`, `random-bytes`) and `extension/notifier.rs`
//! (`destination-settings`, RD-170-09) at commit 26c8592a6.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rand::Rng as _;
use rc_plugin_types::{Failure, FailureKind};

use crate::{
    bindings::rcommander::plugin::{host, settings, sink, source, types},
    failure::readable,
    runtime::PluginStoreState,
    streams::read_window,
};

/// Longest plugin log line kept, in characters.
const MAX_LOG_CHARS: usize = 4096;
/// Most random bytes one `host.random-bytes` call may ask for; above it the answer is empty, so a
/// guest notices instead of working with fewer bytes than it asked for.
const MAX_RANDOM_BYTES: u32 = 1024;

/// The WIT record of a host failure.
pub(crate) fn to_wit_failure(failure: Failure) -> types::Failure {
    types::Failure {
        kind: match failure.kind {
            FailureKind::NotFound => types::FailureKind::NotFound,
            FailureKind::Exists => types::FailureKind::Exists,
            FailureKind::AccessDenied => types::FailureKind::AccessDenied,
            FailureKind::NotSupported => types::FailureKind::NotSupported,
            FailureKind::BadData => types::FailureKind::BadData,
            FailureKind::WrongPassword => types::FailureKind::WrongPassword,
            FailureKind::Cancelled => types::FailureKind::Cancelled,
            FailureKind::Io => types::FailureKind::Io,
            FailureKind::Other => types::FailureKind::Other,
        },
        code: failure.code,
        params: failure.params.into_iter().collect(),
    }
}

impl types::Host for PluginStoreState {}

impl host::Host for PluginStoreState {
    /// At most [`crate::MAX_LOG_LINES_PER_CALL`] lines per call reach the log, each cut to
    /// [`MAX_LOG_CHARS`] with its control and direction characters spelled out, so a plugin can
    /// neither flood the log nor forge a line of rCommander's.
    async fn log(&mut self, level: String, message: String) {
        if !self.log_budget.admit() {
            return;
        }
        let line = readable(&message, MAX_LOG_CHARS);
        let origin = self.log_budget.origin();
        let plugin_id = origin.plugin_id.as_str();
        let operation = origin.operation;
        match level.to_ascii_lowercase().as_str() {
            "error" => tracing::error!(plugin = true, plugin_id, operation, "{line}"),
            "warn" => tracing::warn!(plugin = true, plugin_id, operation, "{line}"),
            "debug" | "trace" => tracing::debug!(plugin = true, plugin_id, operation, "{line}"),
            _ => tracing::info!(plugin = true, plugin_id, operation, "{line}"),
        }
    }

    async fn now_unix_seconds(&mut self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or_default()
    }

    async fn random_bytes(&mut self, count: u32) -> Vec<u8> {
        if count == 0 || count > MAX_RANDOM_BYTES {
            return Vec::new();
        }
        let mut bytes = vec![0_u8; count as usize];
        rand::rng().fill_bytes(&mut bytes);
        bytes
    }
}

impl source::Host for PluginStoreState {
    async fn info(&mut self, src: u32) -> types::SourceInfo {
        let info = self.streams.info(src);
        types::SourceInfo {
            name: info
                .as_ref()
                .map(|info| info.name.clone())
                .unwrap_or_default(),
            size: info.as_ref().map_or(0, |info| info.size),
            modified: info.and_then(|info| info.modified),
        }
    }

    async fn read_at(
        &mut self,
        src: u32,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, types::Failure> {
        let source = self.streams.source(src).map_err(to_wit_failure)?;
        // The backend is waiting, not the plugin thinking (rDownloader's RA-HOST-05).
        let started = Instant::now();
        let read = read_window(source, offset, length).await;
        self.credit_host_time(started.elapsed());
        read.map_err(to_wit_failure)
    }

    async fn progress(&mut self, _done: u64, _total: Option<u64>) {}

    async fn should_stop(&mut self) -> bool {
        self.cancelled()
    }
}

impl sink::Host for PluginStoreState {
    async fn write_at(
        &mut self,
        out: u32,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<(), types::Failure> {
        let started = Instant::now();
        let written = self.streams.write_at(out, offset, &bytes).await;
        self.credit_host_time(started.elapsed());
        written.map_err(to_wit_failure)
    }

    async fn committed(&mut self, out: u32) -> u64 {
        self.streams.committed(out)
    }

    async fn sync(&mut self, out: u32) -> Result<(), types::Failure> {
        let started = Instant::now();
        let synced = self.streams.sync(out).await;
        self.credit_host_time(started.elapsed());
        synced.map_err(to_wit_failure)
    }

    async fn progress(&mut self, _done: u64, _total: Option<u64>) {}

    async fn should_stop(&mut self) -> bool {
        self.cancelled()
    }
}

/// `settings`: the plugin's own declared settings, and nothing else - no other plugin's, none of
/// rCommander's, nothing secret. The store holds them resolved (`ViewerOptions::settings`).
impl settings::Host for PluginStoreState {
    async fn setting(&mut self, name: String) -> Option<String> {
        self.settings
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
    }
}
