//! The inventory kind (RC-100-39): a server inventory (NetBox, Proxmox, ...) compiled and linked,
//! asked for its hosts.
//!
//! Built like the secrets kind (`secrets/mod.rs`): the manifest checked, the component compiled
//! with its imports held to the grant, linked by hand under the manifest's contract version, one
//! fresh store per call. Its `http` reaches only the address the person entered and confirmed for
//! the source ([`InventoryOptions::address`]), and the API token is never handed to it: the
//! plugin names it as a `{{secret:<slot>}}` marker the host fills in on the way out, and every
//! value is masked out of its log lines, its failures and its answers (`http/markers.rs`).

use std::{
    future::Future,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use rc_plugin_types::{Failure, FailureKind};
use secrecy::SecretString;
use url::Url;
use wasmtime::{Store, component::InstancePre, component::Linker};

use crate::{
    ExecutionLog, PluginHealth, PluginIdentity,
    bindings::rcommander::plugin::types,
    failure::{self, codes},
    http::{HttpGrant, loopback_port},
    link::link_secrets,
    manifest::{PluginManifest, PluginType, resolve_settings, secret_slots, validate_manifest},
    runtime::{PluginStoreState, SandboxEngine, http_granted, settings_granted},
    viewer::{Ended, WALL_CLOCK_FACTOR, bounded, settle},
};

mod guest;
mod validate;

use guest::{InventoryGuest, InventoryIndices};
pub use validate::{
    InventoryList, MAX_EXTERNAL_ID_CHARS, MAX_FOLDER_DEPTH, MAX_FOLDER_NAME_CHARS,
    MAX_HOST_TEXT_CHARS, MAX_NOTE_CHARS, MAX_TAG_CHARS, MAX_TAGS,
};

/// What one inventory source decides about the plugin's calls, and where its records and its
/// health live.
#[derive(Clone, Debug, Default)]
pub struct InventoryOptions {
    pub log: Arc<ExecutionLog>,
    /// Shared by every version of the plugin, so a suspension holds across them.
    pub health: Arc<PluginHealth>,
    /// The person's stored values of the plugin's own settings, as for a viewer (RC-100-14).
    pub settings: Vec<(String, String)>,
    /// The address the person entered and confirmed for the source
    /// (`https://netbox.example.net/`): every request goes to its scheme, host and port. `None`
    /// (a conformance run) denies every request.
    pub address: Option<String>,
    /// The values the person stored for the manifest's `secrets` slots; a slot without one is
    /// `plugin_secret_missing` when a marker names it.
    pub secrets: Vec<(String, SecretString)>,
    /// Loopback ports no plugin reaches whatever was entered: rCommander's MCP port.
    pub refused_ports: Vec<u16>,
}

/// An inventory compiled and linked, ready to be asked.
pub struct InventoryPlugin {
    shared: Arc<Shared>,
}

struct Shared {
    identity: PluginIdentity,
    sandbox: SandboxEngine,
    pre: InstancePre<PluginStoreState>,
    indices: InventoryIndices,
    settings: Vec<(String, String)>,
    http: Option<HttpGrant>,
    log: Arc<ExecutionLog>,
    health: Arc<PluginHealth>,
}

impl InventoryPlugin {
    /// Compiles a verified package's component against the inventory world.
    ///
    /// Refused as a secret source is: an invalid manifest or a component of another world
    /// (`plugin_invalid`), an import the manifest does not grant (`plugin_denied`); an entered
    /// address that is not HTTPS (or plain HTTP to the loopback) is `plugin_http_denied`. A
    /// secret for a slot the manifest does not name is dropped. Compiling takes Cranelift's time
    /// on a cache miss; call it off the UI thread.
    pub fn new(
        manifest: &PluginManifest,
        component: &[u8],
        options: InventoryOptions,
    ) -> Result<Self, Failure> {
        let invalid = || failure::coded(FailureKind::BadData, codes::PLUGIN_INVALID);
        validate_manifest(manifest).map_err(|error| {
            tracing::debug!(error = %format!("{error:#}"), "an inventory's manifest is refused");
            invalid()
        })?;
        if manifest.plugin_type != PluginType::Inventory {
            return Err(invalid());
        }
        let sandbox = SandboxEngine::new(manifest.limits, manifest.timeout_milliseconds())
            .map_err(failure::host_failed)?;
        let component = sandbox.compile_component(component, manifest)?;
        let mut linker = Linker::new(sandbox.engine());
        let granted = http_granted(manifest);
        link_secrets(
            &mut linker,
            &manifest.api_version,
            settings_granted(manifest),
            granted,
        )
        .map_err(failure::host_failed)?;
        let pre = linker.instantiate_pre(&component).map_err(|error| {
            tracing::debug!(%error, "an inventory's component does not link");
            failure::coded(FailureKind::AccessDenied, codes::PLUGIN_DENIED)
        })?;
        let indices =
            InventoryIndices::new(&component, &manifest.api_version).ok_or_else(|| {
                tracing::debug!("a component is not an inventory of its manifest's contract");
                invalid()
            })?;
        let slots = secret_slots(manifest).map_err(|_| invalid())?;
        let secrets: Vec<(String, SecretString)> = options
            .secrets
            .into_iter()
            .filter(|(slot, _)| slots.contains(slot))
            .collect();
        let address = options
            .address
            .as_deref()
            .map(Url::parse)
            .transpose()
            .map_err(|_| failure::coded(FailureKind::AccessDenied, codes::PLUGIN_HTTP_DENIED))?;
        let http = match &address {
            // A refused port is refused here, once: no grant, so `http` stays linked and denies
            // every request (RC-080-52).
            Some(address)
                if granted
                    && !loopback_port(address)
                        .is_some_and(|port| options.refused_ports.contains(&port)) =>
            {
                Some(HttpGrant::entered(
                    address,
                    secrets,
                    manifest.limits.max_response_bytes,
                    sandbox.timeout(),
                )?)
            }
            _ => None,
        };
        Ok(Self {
            shared: Arc::new(Shared {
                identity: PluginIdentity {
                    id: manifest.id.clone(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                    plugin_type: "inventory",
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

    /// Every host of the inventory, checked: at most [`rc_plugin_api::inventory::MAX_HOSTS`]
    /// (more is `plugin_inventory_too_large`), each breaking a rule left out and counted, every
    /// stored secret masked out of the text.
    pub async fn list(&self) -> Result<InventoryList, Failure> {
        self.recorded("list", async {
            let (mut store, guest) = self.enter("list").await?;
            let ended = bounded(self.wall_clock(), guest.call_list(&mut store)).await;
            let mask = |text: &str| self.mask(text);
            validate::hosts(settle(self.masked_answer(ended), false, &mut false)?, &mask)
        })
        .await
    }

    /// The plugin's own failure masked before the host reads and cuts it, so a cut never leaves
    /// half a token behind.
    fn masked_answer<T>(
        &self,
        ended: Ended<Result<T, types::Failure>>,
    ) -> Ended<Result<T, types::Failure>> {
        match ended {
            Ended::Returned(Err(mut failure)) => {
                failure.code = failure.code.map(|code| self.mask(&code));
                for (_, value) in &mut failure.params {
                    *value = self.mask(value);
                }
                Ended::Returned(Err(failure))
            }
            other => other,
        }
    }

    /// `text` with every stored secret masked.
    fn mask(&self, text: &str) -> String {
        match &self.shared.http {
            Some(grant) => grant.mask(text).into_owned(),
            None => text.to_owned(),
        }
    }

    /// A failure whose parameters, the plugin's own words among them, carry no stored secret.
    fn masked(&self, mut failure: Failure) -> Failure {
        for value in failure.params.values_mut() {
            *value = self.mask(value);
        }
        failure
    }

    /// The longest the caller waits for one call.
    fn wall_clock(&self) -> Duration {
        self.shared.sandbox.timeout() * WALL_CLOCK_FACTOR
    }

    /// A fresh store for one call, the call begun and the component instantiated in it.
    async fn enter(
        &self,
        operation: &'static str,
    ) -> Result<(Store<PluginStoreState>, InventoryGuest), Failure> {
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

    /// Runs one call unless the plugin is suspended, files its record (masked, as the caller
    /// sees it) and counts a malfunction towards suspension, as a secret source's calls are.
    async fn recorded<T>(
        &self,
        operation: &'static str,
        call: impl Future<Output = Result<T, Failure>>,
    ) -> Result<T, Failure> {
        let shared = &self.shared;
        shared.health.check()?;
        let _running = shared.health.enter(shared.sandbox.max_concurrent()).await;
        let invocation = shared.log.begin(operation);
        let result = call.await.map_err(|failure| self.masked(failure));
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
#[path = "inventory_tests.rs"]
mod tests;
