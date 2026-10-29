//! The viewer kind (Lister, WLX): a compiled viewer plugin and the documents it opens.
//!
//! Adapted from rDownloader's `crates/rd-plugin-host/src/component.rs` (`ComponentResolver::new`:
//! compile, check the imports, link what is granted, pre-instantiate) at commit 26c8592a6. What
//! a viewer returns is structured content rCommander renders itself (`docs/plugins.md`, "Viewer
//! (WLX)"); RC-100-13 connects [`ViewerSession`] to Lister and Quick View.

use std::sync::{Arc, atomic::AtomicBool};

use rc_plugin_api::viewer::{Purpose, Theme};
use rc_plugin_types::{Failure, FailureKind};
use wasmtime::component::Linker;

use crate::{
    ExecutionLog, PluginHealth, PluginIdentity,
    failure::{self, codes},
    link::link_viewer,
    manifest::{PluginManifest, PluginType, resolve_settings, validate_manifest},
    runtime::{SandboxEngine, settings_granted},
    streams::SourceReader,
};

mod call;
mod convert;
mod guest;
mod session;
mod tree;
mod validate;

use call::Shared;
/// The call helpers the secrets kind shares (RC-100-33).
pub(crate) use call::{Ended, WALL_CLOCK_FACTOR, bounded, infallible, settle};
use guest::ViewerIndices;
pub use session::{ImageOutput, MAX_NEEDLE_CHARS, TextOutput, ViewerSession};
pub use tree::{
    MAX_LABEL_CHARS, MAX_TREE_CHILDREN, MAX_TREE_DEPTH, MAX_TREE_NODES, MAX_VALUE_CHARS,
    TreeChildren,
};
pub use validate::{
    ImageFormat, MAX_PAGES, MAX_PIXELS, MAX_SIDE, MAX_TITLE_CHARS, MAX_TOKEN_LINES,
    MAX_TOKENS_PER_LINE, image_format, raster_bytes,
};

/// `plugins.viewer_output_limit_mib`'s default, in bytes (256 MiB).
pub const DEFAULT_OUTPUT_LIMIT_BYTES: u64 = 256 * 1024 * 1024;

/// What the settings decide about a viewer's calls, and where its records and its health live.
#[derive(Clone, Debug)]
pub struct ViewerOptions {
    /// The most bytes one output may write: `plugins.viewer_output_limit_mib`.
    pub output_limit: u64,
    pub log: Arc<ExecutionLog>,
    /// Shared by every version of the plugin, so a suspension holds across them.
    pub health: Arc<PluginHealth>,
    /// The person's stored values of the plugin's own settings, `(key, value)` without the
    /// `plugin.<id>.` prefix. [`ViewerPlugin::new`] keeps those its manifest declares and still
    /// accepts and fills in the defaults; `settings.setting` answers from that in every store
    /// (RC-100-14). A change reaches the next plugin built, so the next document opened.
    pub settings: Vec<(String, String)>,
}

impl Default for ViewerOptions {
    fn default() -> Self {
        Self {
            output_limit: DEFAULT_OUTPUT_LIMIT_BYTES,
            log: Arc::new(ExecutionLog::new()),
            health: Arc::new(PluginHealth::default()),
            settings: Vec::new(),
        }
    }
}

/// A viewer plugin compiled and linked, ready to open documents.
pub struct ViewerPlugin {
    shared: Arc<Shared>,
}

impl ViewerPlugin {
    /// Compiles a verified package's component against the viewer world.
    ///
    /// Refused: a manifest that is not a valid viewer's (`plugin_invalid`), a component that
    /// imports an interface the manifest does not grant or another contract version than its
    /// `api_version` (`plugin_denied`, checked before linking and again by the linker), one that
    /// is not a viewer of that version (`plugin_invalid`). Compiling takes
    /// Cranelift's time on a cache miss; call it off the UI thread.
    pub fn new(
        manifest: &PluginManifest,
        component: &[u8],
        options: ViewerOptions,
    ) -> Result<Self, Failure> {
        let invalid = || failure::coded(FailureKind::BadData, codes::PLUGIN_INVALID);
        validate_manifest(manifest).map_err(|error| {
            tracing::debug!(error = %format!("{error:#}"), "a viewer's manifest is refused");
            invalid()
        })?;
        let Some(viewer) = manifest
            .viewer
            .as_ref()
            .filter(|_| manifest.plugin_type == PluginType::Viewer)
        else {
            return Err(invalid());
        };
        let sandbox = SandboxEngine::new(manifest.limits, manifest.timeout_milliseconds())
            .map_err(failure::host_failed)?;
        let component = sandbox.compile_component(component, manifest)?;
        let mut linker = Linker::new(sandbox.engine());
        link_viewer(
            &mut linker,
            &manifest.api_version,
            settings_granted(manifest),
        )
        .map_err(failure::host_failed)?;
        let pre = linker.instantiate_pre(&component).map_err(|error| {
            tracing::debug!(%error, "a viewer component does not link");
            failure::coded(FailureKind::AccessDenied, codes::PLUGIN_DENIED)
        })?;
        let indices = ViewerIndices::new(&component, &manifest.api_version).ok_or_else(|| {
            tracing::debug!("a component is not a viewer of its manifest's contract");
            invalid()
        })?;
        Ok(Self {
            shared: Arc::new(Shared {
                identity: PluginIdentity {
                    id: manifest.id.clone(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                    plugin_type: "viewer",
                },
                offered: viewer.outputs.clone(),
                sandbox,
                pre,
                indices,
                settings: resolve_settings(&manifest.settings, &options.settings),
                log: options.log,
                health: options.health,
                output_limit: options.output_limit,
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

    /// Asks the plugin whether it shows `source`, in a store of its own. `[match]` has already
    /// said yes; this is the plugin's finer answer.
    pub async fn probe(&self, source: Arc<dyn SourceReader>) -> Result<bool, Failure> {
        self.shared.health.check()?;
        let _running = self.shared.enter().await;
        let invocation = self.shared.log.begin("probe");
        let result = self.probe_in_store(source).await;
        self.shared.finish(invocation, &result);
        result
    }

    async fn probe_in_store(&self, source: Arc<dyn SourceReader>) -> Result<bool, Failure> {
        let shared = &self.shared;
        let mut store = shared.create_store(Arc::new(AtomicBool::new(false)))?;
        shared
            .sandbox
            .begin_call(&mut store, "probe")
            .map_err(failure::host_failed)?;
        let src = store.data_mut().streams.add_source(source);
        let wall = shared.wall_clock();
        let guest = match bounded(wall, shared.instantiate(&mut store)).await {
            Ended::Returned(guest) => guest,
            Ended::Trapped(error) => return Err(failure::stopped(Some(&error), false)),
            Ended::OverTime => return Err(failure::stopped(None, false)),
        };
        let ended = bounded(wall, guest.call_probe(&mut store, src)).await;
        settle(infallible(ended), false, &mut false)
    }

    /// Opens `source` in a store that lives as long as the returned session. `cancel` is the
    /// caller's flag: raised, the running call is asked to stop (`should-stop`) and ended
    /// [`crate::CANCEL_GRACE`] later, as is every later call of the session.
    pub async fn open(
        &self,
        source: Arc<dyn SourceReader>,
        purpose: Purpose,
        theme: &Theme,
        cancel: Arc<AtomicBool>,
    ) -> Result<ViewerSession, Failure> {
        self.shared.health.check()?;
        let _running = self.shared.enter().await;
        let invocation = self.shared.log.begin("open");
        let result = self.open_in_store(source, purpose, theme, cancel).await;
        self.shared.finish(invocation, &result);
        result
    }

    async fn open_in_store(
        &self,
        source: Arc<dyn SourceReader>,
        purpose: Purpose,
        theme: &Theme,
        cancel: Arc<AtomicBool>,
    ) -> Result<ViewerSession, Failure> {
        let shared = &self.shared;
        let mut store = shared.create_store(Arc::clone(&cancel))?;
        shared
            .sandbox
            .begin_call(&mut store, "open")
            .map_err(failure::host_failed)?;
        let src = store.data_mut().streams.add_source(source);
        let wall = shared.wall_clock();
        let guest = match bounded(wall, shared.instantiate(&mut store)).await {
            Ended::Returned(guest) => guest,
            Ended::Trapped(error) => {
                return Err(failure::stopped(Some(&error), store.data().cancelled()));
            }
            Ended::OverTime => return Err(failure::stopped(None, store.data().cancelled())),
        };
        let theme = convert::theme(theme);
        let ended = bounded(
            wall,
            guest.call_open(&mut store, src, convert::purpose(purpose), theme),
        )
        .await;
        let cancelled = store.data().cancelled();
        let raw = settle(ended, cancelled, &mut false)?;
        let document = validate::document(raw, &shared.offered)?;
        Ok(ViewerSession {
            shared: Arc::clone(shared),
            store,
            guest,
            document,
            cancel,
            tree: tree::TreeState::default(),
            closed: false,
        })
    }
}

#[cfg(test)]
#[path = "viewer_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hostile_tests.rs"]
mod hostile_tests;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod settings_tests;
