//! Wasmtime component runtime with deny-by-default imports: the store, its limits and the
//! deadline of each call.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/runtime.rs` at commit
//! 26c8592a6: the same store limits (memory, instances, tables, table elements), fuel, the epoch
//! deadline consulted against a wall-clock instant and host time credited back. Deviations, each
//! from `docs/plugins.md`:
//!
//! - **A store lives for an opened document** ("Long-lived stores"), so fuel, deadline, log
//!   budget and stream credit are set anew by [`SandboxEngine::begin_call`]; the memory limit
//!   holds for the store.
//! - **The epoch callback yields** (`UpdateDeadline::Yield`) instead of continuing, so a busy
//!   plugin hands the tokio worker back every tick and a caller that drops the call (Quick View
//!   moved on) is never held up by it.
//! - **Cancellation**: `should-stop` answers the caller's flag, and a call still running
//!   [`CANCEL_GRACE`] after the flag went up ends at the next tick ("Viewer (WLX)": `should-stop`,
//!   then the epoch).
//! - No resolver state (domains, identity, redactions, wait budget): a viewer reaches no network
//!   and no secret; a secret source reaches one loopback port ([`HttpGrant`], RC-100-33).

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use rc_plugin_types::{Failure, FailureKind};
use wasmtime::{
    Engine, Store, StoreLimits, StoreLimitsBuilder, UpdateDeadline, component::Component,
};

use crate::{
    PluginLimits,
    engine::SharedEngine,
    failure::{self, codes},
    http::HttpGrant,
    log_budget::LogBudget,
    manifest::PluginManifest,
    streams::Streams,
};

mod imports;

pub(crate) use imports::{allowed_imports, http_granted, settings_granted, versioned};

pub(crate) const EPOCH_TICK: Duration = Duration::from_millis(10);
/// Most elements one plugin table may hold (rDownloader's PL-01): table storage is not counted
/// against `memory_bytes`, so without this cap a `table.grow` loop took host memory at will.
pub(crate) const MAX_TABLE_ELEMENTS: usize = 10_000;
/// How long a cancelled call may still run to answer `should-stop` itself.
pub const CANCEL_GRACE: Duration = Duration::from_millis(200);

/// The state of one store, reached only through the host functions linked for its kind.
pub(crate) struct PluginStoreState {
    limits: StoreLimits,
    /// Wall-clock instant at which the running call's compute budget runs out.
    execution_deadline: Instant,
    /// The caller's flag: Quick View moved on, the window closed.
    cancel: Arc<AtomicBool>,
    /// When a cancelled call has to have ended.
    cancel_deadline: Option<Instant>,
    /// The lines the running call may still log.
    pub(crate) log_budget: LogBudget,
    /// The sources of the store and the sinks of the running call.
    pub(crate) streams: Streams,
    /// What `settings.setting` answers: the plugin's declared settings, each the person's value
    /// or the manifest's default, taken when the store was created (RC-100-14).
    pub(crate) settings: Vec<(String, String)>,
    /// What `http` may reach: a secret source's loopback port (RC-100-33); `None` for every
    /// other store.
    pub(crate) http: Option<HttpGrant>,
}

impl PluginStoreState {
    /// Credits time spent inside a host call back to the deadline: waiting on a slow backend is
    /// not the plugin thinking (rDownloader's RA-HOST-05). Fuel still runs out on a spinning one.
    pub(crate) fn credit_host_time(&mut self, elapsed: Duration) {
        self.execution_deadline += elapsed;
    }

    /// Whether the caller asked the running call to stop.
    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Compiles components and creates resource-limited stores.
///
/// Cheap to clone: the engine behind it is the process's one shared engine ([`crate::engine`]);
/// what is per plugin here is the limits its stores are given.
#[derive(Clone)]
pub(crate) struct SandboxEngine {
    shared: Arc<SharedEngine>,
    limits: PluginLimits,
    timeout: Duration,
}

impl SandboxEngine {
    /// A sandbox on the shared engine; `timeout_milliseconds` is the compute time of one call.
    pub(crate) fn new(limits: PluginLimits, timeout_milliseconds: u64) -> Result<Self> {
        limits.check().map_err(anyhow::Error::msg)?;
        if timeout_milliseconds == 0 {
            anyhow::bail!("plugin timeout must be non-zero");
        }
        Ok(Self {
            shared: crate::engine::shared()?,
            limits,
            timeout: Duration::from_millis(timeout_milliseconds),
        })
    }

    /// The compute time of one call.
    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }

    /// How many calls of the plugin may run at once (`limits.max_concurrent`).
    pub(crate) fn max_concurrent(&self) -> u32 {
        self.limits.max_concurrent
    }

    /// Compiles one component and refuses every import the manifest does not grant.
    ///
    /// The first of the two places a grant is enforced - the linker is the second. A package
    /// that reaches beyond its manifest is refused where its author sees it, before any call;
    /// so is one importing another contract version than its manifest's `api_version`.
    pub(crate) fn compile_component(
        &self,
        bytes: &[u8],
        manifest: &PluginManifest,
    ) -> Result<Component, Failure> {
        // Compiled once per content; the grant is checked every time, because the same bytes
        // may arrive under a different manifest.
        let component = self.shared.compile(bytes).map_err(|error| {
            tracing::debug!(%error, "a plugin component does not compile");
            failure::coded(FailureKind::BadData, codes::PLUGIN_INVALID)
        })?;
        let allowed = allowed_imports(manifest);
        for (name, _) in component.component_type().imports(self.engine()) {
            if !allowed.iter().any(|allowed| allowed == name) {
                return Err(
                    failure::coded(FailureKind::AccessDenied, codes::PLUGIN_DENIED)
                        .with_param("import", failure::readable(name, 128)),
                );
            }
        }
        Ok(component)
    }

    /// A fresh store for one plugin, its log lines named after `plugin_id` and `version`.
    pub(crate) fn create_store(
        &self,
        plugin_id: &str,
        version: &str,
        cancel: Arc<AtomicBool>,
    ) -> Result<Store<PluginStoreState>> {
        let memory_bytes = usize::try_from(self.limits.memory_bytes)
            .context("plugin memory limit exceeds platform usize")?;
        let state = PluginStoreState {
            limits: StoreLimitsBuilder::new()
                .memory_size(memory_bytes)
                .instances(32)
                .tables(8)
                .table_elements(MAX_TABLE_ELEMENTS)
                .memories(1)
                .trap_on_grow_failure(true)
                .build(),
            execution_deadline: Instant::now() + self.timeout,
            cancel,
            cancel_deadline: None,
            log_budget: LogBudget::new(plugin_id.to_owned(), version.to_owned()),
            streams: Streams::default(),
            settings: Vec::new(),
            http: None,
        };
        let mut store = Store::new(self.engine(), state);
        store.limiter(|state| &mut state.limits);
        store.epoch_deadline_callback(|mut context| {
            let now = Instant::now();
            let state = context.data_mut();
            if state.cancelled() {
                let grace = *state.cancel_deadline.get_or_insert(now + CANCEL_GRACE);
                if now >= grace {
                    return Ok(UpdateDeadline::Interrupt);
                }
            }
            if now >= state.execution_deadline {
                return Ok(UpdateDeadline::Interrupt);
            }
            Ok(UpdateDeadline::Yield(1))
        });
        Ok(store)
    }

    /// Starts one call of `operation` in `store`: its fuel, its deadline, its log budget and its
    /// stream credit; no sink of an earlier call stays.
    pub(crate) fn begin_call(
        &self,
        store: &mut Store<PluginStoreState>,
        operation: &'static str,
    ) -> Result<()> {
        store
            .set_fuel(self.limits.fuel)
            .map_err(|error| anyhow::anyhow!("configure plugin fuel: {error}"))?;
        store.set_epoch_deadline(1);
        let state = store.data_mut();
        state.execution_deadline = Instant::now() + self.timeout;
        state.cancel_deadline = None;
        state.log_budget.begin(operation);
        state.streams.begin_call();
        Ok(())
    }

    /// The underlying engine, for the linkers.
    pub(crate) fn engine(&self) -> &Engine {
        self.shared.engine()
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
