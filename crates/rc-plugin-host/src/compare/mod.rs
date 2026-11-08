//! The compare kind (RC-100-41): a plugin that compares two files of the kinds its `[match]`
//! claims by structure - JSON, CSV - into a list of changes beside the diff window's line diff.
//!
//! Built like the inventory kind (`inventory/mod.rs`): the manifest checked, the component
//! compiled with its imports held to the grant, linked by hand under the manifest's contract
//! version, one fresh store per call. The two files are the call's sources and nothing else; the
//! plugin writes nothing and reaches nothing. What it answers is checked by `validate.rs`.

use std::{
    future::Future,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use rc_plugin_api::compare::{CompareState, Comparison};
use rc_plugin_types::{Failure, FailureKind};
use wasmtime::{Store, component::InstancePre, component::Linker};

use crate::{
    ExecutionLog, MatchSection, PluginHealth, PluginIdentity,
    failure::{self, codes},
    link::link_compare,
    manifest::{PluginManifest, PluginType, resolve_settings, validate_manifest},
    runtime::{PluginStoreState, SandboxEngine, settings_granted},
    streams::SourceReader,
    viewer::{Ended, WALL_CLOCK_FACTOR, bounded, infallible, settle},
};

mod guest;
mod validate;

use guest::{CompareGuest, CompareIndices};
pub use validate::{MAX_CHANGE_VALUE_CHARS, MAX_COLUMN_CHARS, MAX_PATH_CHARS};

/// What the engine decides about a compare plugin's calls, and where its records and its health
/// live.
#[derive(Clone, Debug, Default)]
pub struct CompareOptions {
    pub log: Arc<ExecutionLog>,
    /// Shared by every version of the plugin, so a suspension holds across them.
    pub health: Arc<PluginHealth>,
    /// The person's stored values of the plugin's own settings, as for a viewer (RC-100-14): the
    /// licence key, the array key, the key columns.
    pub settings: Vec<(String, String)>,
}

/// A compare plugin compiled and linked, ready to compare.
pub struct ComparePlugin {
    shared: Arc<Shared>,
}

struct Shared {
    identity: PluginIdentity,
    matching: MatchSection,
    sandbox: SandboxEngine,
    pre: InstancePre<PluginStoreState>,
    indices: CompareIndices,
    settings: Vec<(String, String)>,
    log: Arc<ExecutionLog>,
    health: Arc<PluginHealth>,
}

impl ComparePlugin {
    /// Compiles a verified package's component against the compare world.
    ///
    /// Refused as an inventory is: an invalid manifest or a component of another world
    /// (`plugin_invalid`), an import the manifest does not grant (`plugin_denied`). Compiling
    /// takes Cranelift's time on a cache miss; call it off the UI thread.
    pub fn new(
        manifest: &PluginManifest,
        component: &[u8],
        options: CompareOptions,
    ) -> Result<Self, Failure> {
        let invalid = || failure::coded(FailureKind::BadData, codes::PLUGIN_INVALID);
        validate_manifest(manifest).map_err(|error| {
            tracing::debug!(error = %format!("{error:#}"), "a compare plugin's manifest is refused");
            invalid()
        })?;
        if manifest.plugin_type != PluginType::Compare {
            return Err(invalid());
        }
        let sandbox = SandboxEngine::new(manifest.limits, manifest.timeout_milliseconds())
            .map_err(failure::host_failed)?;
        let component = sandbox.compile_component(component, manifest)?;
        let mut linker = Linker::new(sandbox.engine());
        link_compare(
            &mut linker,
            &manifest.api_version,
            settings_granted(manifest),
        )
        .map_err(failure::host_failed)?;
        let pre = linker.instantiate_pre(&component).map_err(|error| {
            tracing::debug!(%error, "a compare plugin's component does not link");
            failure::coded(FailureKind::AccessDenied, codes::PLUGIN_DENIED)
        })?;
        let indices = CompareIndices::new(&component, &manifest.api_version).ok_or_else(|| {
            tracing::debug!("a component is not a compare plugin of its manifest's contract");
            invalid()
        })?;
        Ok(Self {
            shared: Arc::new(Shared {
                identity: PluginIdentity {
                    id: manifest.id.clone(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                    plugin_type: "compare",
                },
                matching: manifest.matching.clone(),
                sandbox,
                pre,
                indices,
                settings: resolve_settings(&manifest.settings, &options.settings),
                log: options.log,
                health: options.health,
            }),
        })
    }

    #[must_use]
    pub fn identity(&self) -> &PluginIdentity {
        &self.shared.identity
    }

    /// The failure count and suspension; the person's Resume is [`PluginHealth::resume`].
    #[must_use]
    pub fn health(&self) -> &Arc<PluginHealth> {
        &self.shared.health
    }

    #[must_use]
    pub fn execution_log(&self) -> &Arc<ExecutionLog> {
        &self.shared.log
    }

    /// Whether the plugin compares a file named `file_name` of `size` bytes: its `[match]`
    /// names the extension or the name, and the file is not larger than its `max_size`.
    #[must_use]
    pub fn claims(&self, file_name: &str, size: u64) -> bool {
        self.shared.matching.matches(file_name, &[], size)
    }

    /// Whether it compares now: a paid plugin without a valid licence answers `unlicensed`.
    pub async fn status(&self) -> Result<CompareState, Failure> {
        self.recorded("status", async {
            let (mut store, guest, _) = self.enter("status", Vec::new()).await?;
            let ended = bounded(self.wall_clock(), guest.call_status(&mut store)).await;
            settle(infallible(ended), false, &mut false).map(validate::state)
        })
        .await
    }

    /// The changes from `a` (the old, left file) to `b` (the new, right one), checked: at most
    /// [`rc_plugin_api::compare::MAX_CHANGES`] (more is `plugin_compare_too_large`), every text
    /// cut. `cancel` raised asks the plugin to stop (`should-stop`) and ends the call
    /// [`crate::CANCEL_GRACE`] later.
    pub async fn compare(
        &self,
        a: Arc<dyn SourceReader>,
        b: Arc<dyn SourceReader>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Comparison, Failure> {
        self.recorded("compare", async {
            let (mut store, guest, handles) = self
                .enter_cancellable("compare", vec![a, b], cancel)
                .await?;
            let [a, b] = handles[..] else {
                return Err(failure::host_failed("two sources were not offered"));
            };
            let ended = bounded(self.wall_clock(), guest.call_compare(&mut store, a, b)).await;
            let cancelled = store.data().cancelled();
            validate::comparison(settle(ended, cancelled, &mut false)?)
        })
        .await
    }

    /// The longest the caller waits for one call.
    fn wall_clock(&self) -> Duration {
        self.shared.sandbox.timeout() * WALL_CLOCK_FACTOR
    }

    async fn enter(
        &self,
        operation: &'static str,
        sources: Vec<Arc<dyn SourceReader>>,
    ) -> Result<(Store<PluginStoreState>, CompareGuest, Vec<u32>), Failure> {
        self.enter_cancellable(operation, sources, Arc::new(AtomicBool::new(false)))
            .await
    }

    /// A fresh store for one call, `sources` offered in it (their handles answered in order),
    /// the call begun and the component instantiated in it.
    async fn enter_cancellable(
        &self,
        operation: &'static str,
        sources: Vec<Arc<dyn SourceReader>>,
        cancel: Arc<AtomicBool>,
    ) -> Result<(Store<PluginStoreState>, CompareGuest, Vec<u32>), Failure> {
        let shared = &self.shared;
        let mut store = shared
            .sandbox
            .create_store(&shared.identity.id, &shared.identity.version, cancel)
            .map_err(failure::host_failed)?;
        store.data_mut().settings = shared.settings.clone();
        shared
            .sandbox
            .begin_call(&mut store, operation)
            .map_err(failure::host_failed)?;
        let handles: Vec<u32> = sources
            .into_iter()
            .map(|source| store.data_mut().streams.add_source(source))
            .collect();
        let instantiated = async {
            let instance = shared.pre.instantiate_async(&mut store).await?;
            shared.indices.load(&mut store, &instance)
        };
        match bounded(self.wall_clock(), instantiated).await {
            Ended::Returned(guest) => Ok((store, guest, handles)),
            Ended::Trapped(error) => Err(failure::stopped(Some(&error), store.data().cancelled())),
            Ended::OverTime => Err(failure::stopped(None, store.data().cancelled())),
        }
    }

    /// Runs one call unless the plugin is suspended, files its record and counts a malfunction
    /// towards suspension, as an inventory's calls are.
    async fn recorded<T>(
        &self,
        operation: &'static str,
        call: impl Future<Output = Result<T, Failure>>,
    ) -> Result<T, Failure> {
        let shared = &self.shared;
        shared.health.check()?;
        let _running = shared.health.enter(shared.sandbox.max_concurrent()).await;
        let invocation = shared.log.begin(operation);
        let result = call.await;
        let outcome = shared.log.finish(&shared.identity, invocation, &result);
        if outcome.counts_as_failure() && shared.health.record_failure() {
            tracing::warn!(
                code = "plugin_suspended",
                plugin_id = shared.identity.id.as_str(),
                plugin_version = shared.identity.version.as_str(),
                "a plugin failed repeatedly and is suspended until it is resumed"
            );
        }
        result
    }
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
