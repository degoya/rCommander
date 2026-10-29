//! One call into a viewer: bounded by the caller's wall clock, its answer settled into a result,
//! its record filed and its failure counted.
//!
//! Adapted from rDownloader's `crates/rd-plugin-host/src/component.rs` (`recorded`, `bindings`,
//! `component_failure`) at commit 26c8592a6.

use std::{
    future::Future,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use rc_plugin_api::viewer::Output;
use rc_plugin_types::Failure;
use wasmtime::{Store, component::InstancePre};

use super::convert;
use super::guest::{ViewerGuest, ViewerIndices};
use crate::{
    CallSlot, ExecutionLog, Invocation, PluginHealth, PluginIdentity,
    bindings::rcommander::plugin::types,
    failure,
    runtime::{PluginStoreState, SandboxEngine},
};

/// How many times its compute budget a call may take on the caller's clock. The epoch deadline
/// ends a busy plugin; this ends one parked in a host call past the time credited back to it
/// (a backend that stopped answering), so the caller never waits without bound.
pub(crate) const WALL_CLOCK_FACTOR: u32 = 3;

/// What one plugin is to its sessions: who it is, what it may offer, how it runs.
pub(crate) struct Shared {
    pub(crate) identity: PluginIdentity,
    /// The manifest's `[viewer] outputs`.
    pub(crate) offered: Vec<Output>,
    pub(crate) sandbox: SandboxEngine,
    pub(crate) pre: InstancePre<PluginStoreState>,
    /// The functions of `interface viewer` under the manifest's contract version.
    pub(crate) indices: ViewerIndices,
    /// What `settings.setting` answers in every store of this plugin (RC-100-14).
    pub(crate) settings: Vec<(String, String)>,
    pub(crate) log: Arc<ExecutionLog>,
    pub(crate) health: Arc<PluginHealth>,
    /// `plugins.viewer_output_limit_mib`, in bytes.
    pub(crate) output_limit: u64,
}

impl Shared {
    /// The longest the caller waits for one call.
    pub(crate) fn wall_clock(&self) -> Duration {
        self.sandbox.timeout() * WALL_CLOCK_FACTOR
    }

    /// Waits for one of the plugin's `limits.max_concurrent` places; the call runs while the slot
    /// is held (RC-080-43).
    pub(crate) async fn enter(&self) -> CallSlot {
        self.health.enter(self.sandbox.max_concurrent()).await
    }

    /// A fresh store for one call or document: the plugin's limits, `cancel`, its settings.
    pub(crate) fn create_store(
        &self,
        cancel: Arc<AtomicBool>,
    ) -> Result<Store<PluginStoreState>, Failure> {
        let mut store = self
            .sandbox
            .create_store(&self.identity.id, &self.identity.version, cancel)
            .map_err(failure::host_failed)?;
        store.data_mut().settings = self.settings.clone();
        Ok(store)
    }

    /// Instantiates the component in `store` and types its viewer functions.
    pub(crate) async fn instantiate(
        &self,
        store: &mut Store<PluginStoreState>,
    ) -> wasmtime::Result<ViewerGuest> {
        let instance = self.pre.instantiate_async(&mut *store).await?;
        self.indices.load(store, &instance)
    }

    /// Files a finished call and counts a malfunction towards suspension.
    pub(crate) fn finish<T>(&self, invocation: Invocation, result: &Result<T, Failure>) {
        let outcome = self.log.finish(&self.identity, invocation, result);
        if outcome.counts_as_failure() && self.health.record_failure() {
            tracing::warn!(
                code = "plugin_suspended",
                plugin_id = self.identity.id.as_str(),
                plugin_version = self.identity.version.as_str(),
                "a plugin failed repeatedly and is suspended until it is resumed"
            );
        }
    }
}

/// How a call into the guest ended.
pub(crate) enum Ended<T> {
    Returned(T),
    /// A trap, exhausted fuel, the epoch deadline.
    Trapped(wasmtime::Error),
    /// The caller's wall clock ran out first; the call was dropped.
    OverTime,
}

/// Runs `call` for at most `wall`.
pub(crate) async fn bounded<T>(
    wall: Duration,
    call: impl Future<Output = wasmtime::Result<T>>,
) -> Ended<T> {
    match tokio::time::timeout(wall, call).await {
        Ok(Ok(value)) => Ended::Returned(value),
        Ok(Err(error)) => Ended::Trapped(error),
        Err(_) => Ended::OverTime,
    }
}

/// An export that answers no WIT result, made one.
pub(crate) fn infallible<T>(ended: Ended<T>) -> Ended<Result<T, types::Failure>> {
    match ended {
        Ended::Returned(value) => Ended::Returned(Ok(value)),
        Ended::Trapped(error) => Ended::Trapped(error),
        Ended::OverTime => Ended::OverTime,
    }
}

/// The answer of a call, or what stopped it. `stopped` is set when the instance can no longer be
/// entered: after a trap it is in an unknown state, and the document has to be opened again.
pub(crate) fn settle<T>(
    ended: Ended<Result<T, types::Failure>>,
    cancelled: bool,
    stopped: &mut bool,
) -> Result<T, Failure> {
    match ended {
        Ended::Returned(Ok(value)) => Ok(value),
        Ended::Returned(Err(failure)) => Err(convert::from_wit_failure(failure)),
        Ended::Trapped(error) => {
            *stopped = true;
            Err(failure::stopped(Some(&error), cancelled))
        }
        Ended::OverTime => {
            *stopped = true;
            Err(failure::stopped(None, cancelled))
        }
    }
}
