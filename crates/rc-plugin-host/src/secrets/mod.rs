//! The secrets kind (RC-100-33): a password manager compiled and linked, asked whether it
//! answers, for the entries matching a query and for the secret of one entry.
//!
//! Built like the viewer kind (`viewer/mod.rs`): the manifest checked, the component compiled with
//! its imports held to the grant, linked by hand under the manifest's contract version, one fresh
//! store per call. A store lives for one call, so a source keeps nothing between them. The value
//! `get` answers leaves this module only as a [`SecretString`]; who it goes to and that it is
//! scrubbed from every answer afterwards is the engine's (RC-100-34).

use std::{
    future::Future,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use rc_plugin_api::secrets::{SecretEntry, SecretField, SourceState};
use rc_plugin_types::{Failure, FailureKind};
use secrecy::SecretString;
use wasmtime::{Store, component::InstancePre, component::Linker};

use crate::{
    ExecutionLog, PluginHealth, PluginIdentity,
    failure::{self, codes},
    http::HttpGrant,
    link::link_secrets,
    manifest::{PluginManifest, PluginType, net_http, resolve_settings, validate_manifest},
    runtime::{PluginStoreState, SandboxEngine, http_granted, settings_granted},
    viewer::{Ended, WALL_CLOCK_FACTOR, bounded, infallible, settle},
};

mod guest;
mod validate;

use guest::{SecretsGuest, SecretsIndices};
pub use validate::{
    MAX_ENTRY_ID_CHARS, MAX_ENTRY_TEXT_CHARS, MAX_ENTRY_TITLE_CHARS, MAX_QUERY_CHARS,
    MAX_SEARCH_ENTRIES, MAX_SECRET_BYTES,
};

/// What the settings decide about a secret source's calls, and where its records and its health
/// live.
#[derive(Clone, Debug, Default)]
pub struct SecretsOptions {
    pub log: Arc<ExecutionLog>,
    /// Shared by every version of the plugin, so a suspension holds across them.
    pub health: Arc<PluginHealth>,
    /// The person's stored values of the plugin's own settings, as for a viewer (RC-100-14).
    pub settings: Vec<(String, String)>,
    /// Loopback ports no plugin reaches whatever its manifest grants: rCommander's MCP port.
    pub refused_ports: Vec<u16>,
}

/// A secret source compiled and linked, ready to be asked.
pub struct SecretsPlugin {
    shared: Arc<Shared>,
}

struct Shared {
    identity: PluginIdentity,
    sandbox: SandboxEngine,
    pre: InstancePre<PluginStoreState>,
    indices: SecretsIndices,
    settings: Vec<(String, String)>,
    http: Option<HttpGrant>,
    log: Arc<ExecutionLog>,
    health: Arc<PluginHealth>,
}

impl SecretsPlugin {
    /// Compiles a verified package's component against the secrets world.
    ///
    /// Refused as a viewer is (`viewer/mod.rs`): an invalid manifest or a component of another
    /// world (`plugin_invalid`), an import the manifest does not grant (`plugin_denied`).
    /// Compiling takes Cranelift's time on a cache miss; call it off the UI thread.
    pub fn new(
        manifest: &PluginManifest,
        component: &[u8],
        options: SecretsOptions,
    ) -> Result<Self, Failure> {
        let invalid = || failure::coded(FailureKind::BadData, codes::PLUGIN_INVALID);
        validate_manifest(manifest).map_err(|error| {
            tracing::debug!(error = %format!("{error:#}"), "a secret source's manifest is refused");
            invalid()
        })?;
        if manifest.plugin_type != PluginType::Secrets {
            return Err(invalid());
        }
        let sandbox = SandboxEngine::new(manifest.limits, manifest.timeout_milliseconds())
            .map_err(failure::host_failed)?;
        let component = sandbox.compile_component(component, manifest)?;
        let mut linker = Linker::new(sandbox.engine());
        link_secrets(
            &mut linker,
            &manifest.api_version,
            settings_granted(manifest),
            http_granted(manifest),
        )
        .map_err(failure::host_failed)?;
        let pre = linker.instantiate_pre(&component).map_err(|error| {
            tracing::debug!(%error, "a secret source's component does not link");
            failure::coded(FailureKind::AccessDenied, codes::PLUGIN_DENIED)
        })?;
        let indices = SecretsIndices::new(&component, &manifest.api_version).ok_or_else(|| {
            tracing::debug!("a component is not a secret source of its manifest's contract");
            invalid()
        })?;
        let http = net_http(manifest)
            .map_err(|_| invalid())?
            // A port no plugin reaches is refused here, once: no grant, so `http` stays linked
            // and denies every request (RC-080-52).
            .filter(|grant| !options.refused_ports.contains(&grant.loopback))
            .map(|grant| {
                HttpGrant::new(
                    grant.loopback,
                    manifest.limits.max_response_bytes,
                    sandbox.timeout(),
                )
            })
            .transpose()?;
        Ok(Self {
            shared: Arc::new(Shared {
                identity: PluginIdentity {
                    id: manifest.id.clone(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                    plugin_type: "secrets",
                },
                sandbox,
                pre,
                indices,
                settings: resolve_settings(&manifest.settings, &options.settings),
                http,
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

    /// Whether the manager answers now: ready, locked, unreachable, or the plugin unlicensed.
    pub async fn status(&self) -> Result<SourceState, Failure> {
        self.recorded("status", async {
            let (mut store, guest) = self.enter("status").await?;
            let ended = bounded(self.wall_clock(), guest.call_status(&mut store)).await;
            settle(infallible(ended), false, &mut false).map(validate::state)
        })
        .await
    }

    /// At most `limit` (and [`MAX_SEARCH_ENTRIES`]) entries matching `query`: titles, users and
    /// addresses, never a value.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SecretEntry>, Failure> {
        if query.chars().count() > MAX_QUERY_CHARS {
            return Err(failure::coded(
                FailureKind::BadData,
                codes::PLUGIN_REQUEST_TOO_LARGE,
            ));
        }
        let limit = limit.min(MAX_SEARCH_ENTRIES);
        self.recorded("search", async {
            let (mut store, guest) = self.enter("search").await?;
            let ended = bounded(
                self.wall_clock(),
                guest.call_search(&mut store, query, limit),
            )
            .await;
            validate::entries(settle(ended, false, &mut false)?, limit)
        })
        .await
    }

    /// The secret `field` of entry `id`, an id a search handed out.
    pub async fn get(&self, id: &str, field: SecretField) -> Result<SecretString, Failure> {
        if !validate::entry_id(id) {
            return Err(failure::coded(
                FailureKind::BadData,
                codes::PLUGIN_REQUEST_TOO_LARGE,
            ));
        }
        self.recorded("get", async {
            let (mut store, guest) = self.enter("get").await?;
            let ended = bounded(
                self.wall_clock(),
                guest.call_get(&mut store, id, validate::field(field)),
            )
            .await;
            validate::secret(settle(ended, false, &mut false)?)
        })
        .await
    }

    /// The longest the caller waits for one call.
    fn wall_clock(&self) -> Duration {
        self.shared.sandbox.timeout() * WALL_CLOCK_FACTOR
    }

    /// A fresh store for one call, the call begun and the component instantiated in it.
    async fn enter(
        &self,
        operation: &'static str,
    ) -> Result<(Store<PluginStoreState>, SecretsGuest), Failure> {
        let shared = &self.shared;
        let mut store = shared
            .sandbox
            .create_store(
                &shared.identity.id,
                &shared.identity.version,
                Arc::new(AtomicBool::new(false)),
            )
            .map_err(failure::host_failed)?;
        store.data_mut().settings = shared.settings.clone();
        store.data_mut().http = shared.http.clone();
        shared
            .sandbox
            .begin_call(&mut store, operation)
            .map_err(failure::host_failed)?;
        let instantiated = async {
            let instance = shared.pre.instantiate_async(&mut store).await?;
            shared.indices.load(&mut store, &instance)
        };
        match bounded(self.wall_clock(), instantiated).await {
            Ended::Returned(guest) => Ok((store, guest)),
            Ended::Trapped(error) => Err(failure::stopped(Some(&error), false)),
            Ended::OverTime => Err(failure::stopped(None, false)),
        }
    }

    /// Runs one call unless the plugin is suspended, files its record and counts a malfunction
    /// towards suspension, as a viewer's calls are.
    async fn recorded<T>(
        &self,
        operation: &'static str,
        call: impl Future<Output = Result<T, Failure>>,
    ) -> Result<T, Failure> {
        let shared = &self.shared;
        shared.health.check()?;
        // The call's store is made and its budget begun only once a place is free (RC-080-43).
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
#[path = "secrets_tests.rs"]
mod tests;
