//! Secrets a connection takes from a source instead of the vault (RC-100-34).
//!
//! A reference (`rc_db::SecretSourceRef`, migration 0017) names the source - an installed,
//! enabled secrets plugin as `plugin:<id>` (RC-100-33), or `wincred`, the Windows credential
//! manager built into the core (RC-100-37, `secret_sources/wincred.rs`) - and the entry in it.
//! [`Engine::resolve_secret`] asks the reference first, the vault only when there is none.
//!
//! The source is asked at every connect: nothing is cached, the plugin is built for the call
//! (its compiled component comes from the compile cache) and dropped after it. The value goes on
//! the scrub list ([`Engine::remember_secret`]) and lives as a `SecretString` until the login has
//! it. When `get` fails the source is asked for its `status`, so the person reads why: locked
//! (unlock it in the manager - rCommander never asks for its master password), unreachable,
//! unlicensed, or the plugin's own failure code. The host bounds every call (RC-100-33), so a
//! source that hangs fails the connect instead of holding it.
//!
//! The commands are in `secret_sources/commands.rs`: listing the sources and searching one are
//! reads (titles, users and addresses, never a value); setting or removing a reference is the
//! window's alone (`GuiOnly`).

mod commands;
mod error;
mod wincred;

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
};

use rc_plugin_api::secrets::{SecretEntry, SecretField, SourceState};
use rc_plugin_host::{
    ExecutionLog, InstalledPlugin, PluginType, SecretsOptions, SecretsPlugin, installed,
};
use rc_plugin_types::{Failure, FailureKind};
use rc_vault::SecretString;

pub(crate) use commands::check_reference;
pub use commands::{SecretSourceInfo, SecretSourcesOutcome, SourceEntry};
pub use error::SecretSourceError;
pub use rc_db::SecretSourceRef;
pub use wincred::WINCRED_SOURCE_KEY;

use crate::{
    Engine, EngineError, SecretKind,
    plugins::{PluginSettings, stored_plugin_settings},
};

/// The prefix of a plugin source's name; the plugin's id follows.
pub const PLUGIN_SOURCE_PREFIX: &str = "plugin:";

/// The Windows credential manager's source name (RC-100-37).
pub const WINCRED_SOURCE: &str = "wincred";

/// The loopback port rCommander's MCP server listens on in this process; no secret source reaches
/// it, whatever its manifest grants. 0 while no server runs.
static MCP_PORT: AtomicU16 = AtomicU16::new(0);

/// Notes the port the MCP server bound, so no secret source's `http` reaches it.
pub fn note_mcp_port(port: u16) {
    MCP_PORT.store(port, Ordering::Relaxed);
}

/// The plugin field a secret of `kind` is read from.
fn field(kind: SecretKind) -> SecretField {
    match kind {
        SecretKind::Password => SecretField::Password,
        SecretKind::Key => SecretField::PrivateKey,
        SecretKind::Passphrase => SecretField::Passphrase,
    }
}

/// A source built for one call, with the name the person knows it by.
pub(crate) struct LoadedSource {
    pub(crate) name: String,
    backend: Backend,
}

/// What answers for a source: a plugin, or the Windows credential manager.
enum Backend {
    Plugin(SecretsPlugin),
    Wincred,
}

// The plugin's futures are boxed: inline they push the layout of the engine's dispatch future
// past the compiler's query depth (`rc-mcp`).
impl LoadedSource {
    /// Whether the source can hand out secrets now; the credential manager always can.
    pub(crate) async fn status(&self) -> Result<SourceState, Failure> {
        match &self.backend {
            Backend::Plugin(plugin) => Box::pin(plugin.status()).await,
            Backend::Wincred => Ok(SourceState::Ready),
        }
    }

    /// At most `limit` entries matching `query`.
    pub(crate) async fn search(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SecretEntry>, Failure> {
        match &self.backend {
            Backend::Plugin(plugin) => Box::pin(plugin.search(query, limit)).await,
            Backend::Wincred => {
                let query = query.to_owned();
                blocking(move || wincred::search(&query, limit)).await
            }
        }
    }

    /// The secret `kind` of entry `id`. A credential holds one secret: every kind reads it.
    async fn get(&self, id: &str, kind: SecretKind) -> Result<SecretString, Failure> {
        match &self.backend {
            Backend::Plugin(plugin) => Box::pin(plugin.get(id, field(kind))).await,
            Backend::Wincred => {
                let id = id.to_owned();
                blocking(move || wincred::get(&id)).await
            }
        }
    }
}

/// `call` on the blocking pool: the credential functions are system calls.
async fn blocking<T: Send + 'static>(
    call: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(call)
        .await
        .unwrap_or_else(|_| Err(Failure::new(FailureKind::Other)))
}

impl Engine {
    /// The value of the secret `reference` points to, asked from its source now.
    pub(crate) async fn secret_from_source(
        &self,
        reference: &SecretSourceRef,
    ) -> Result<SecretString, EngineError> {
        let source = self.load_source(&reference.source).await?;
        match source.get(&reference.entry_id, reference.kind).await {
            Ok(value) => {
                self.remember_secret(&value);
                Ok(value)
            }
            Err(failure) => {
                let code = failure.code().unwrap_or("plugin_failed").to_owned();
                tracing::debug!(
                    connection = %reference.connection_id,
                    kind = reference.kind.as_str(),
                    code,
                    "a secret source did not hand out the secret"
                );
                let state = source.status().await.ok();
                Err(SecretSourceError::from_state(source.name, state, failure.kind, code).into())
            }
        }
    }

    /// The installed, enabled secrets plugins, each with its manifest; none while
    /// `plugins.enabled` is off or before the engine knows its plugins.
    pub(crate) async fn installed_sources(&self) -> Vec<InstalledPlugin> {
        let settings = PluginSettings::read(self.db().ok()).await;
        let Some(data_dir) = self
            .plugins_data_dir()
            .filter(|_| settings.enabled)
            .map(Path::to_path_buf)
        else {
            return Vec::new();
        };
        match tokio::task::spawn_blocking(move || installed::list(&data_dir)).await {
            Ok(Ok(packages)) => packages
                .into_iter()
                .filter(|package| {
                    package.enabled && package.manifest.plugin_type == PluginType::Secrets
                })
                .collect(),
            Ok(Err(error)) => {
                tracing::warn!(error = %format!("{error:#}"), "the installed plugins could not be listed");
                Vec::new()
            }
            Err(error) => {
                tracing::warn!(%error, "the installed plugins could not be listed");
                Vec::new()
            }
        }
    }

    /// Whether the Windows credential manager is offered: on Windows, while
    /// `vault.wincred_source` is on.
    pub(crate) async fn wincred_offered(&self) -> bool {
        wincred::PLATFORM && self.setting_on(WINCRED_SOURCE_KEY).await
    }

    /// The Windows credential manager as a loaded source.
    pub(crate) fn wincred_source() -> LoadedSource {
        LoadedSource {
            name: wincred::NAME.to_owned(),
            backend: Backend::Wincred,
        }
    }

    /// The source named `source`, built for one call.
    pub(crate) async fn load_source(&self, source: &str) -> Result<LoadedSource, EngineError> {
        let missing = || SecretSourceError::Missing {
            name: source.to_owned(),
        };
        if source == WINCRED_SOURCE {
            return if self.wincred_offered().await {
                Ok(Self::wincred_source())
            } else {
                Err(missing().into())
            };
        }
        let id = source
            .strip_prefix(PLUGIN_SOURCE_PREFIX)
            .ok_or_else(missing)?;
        let package = self
            .installed_sources()
            .await
            .into_iter()
            .find(|package| package.manifest.id == id)
            .ok_or_else(missing)?;
        self.build_source(package).await
    }

    /// `package` compiled as a secret source, with the engine's log, its health and settings.
    pub(crate) async fn build_source(
        &self,
        package: InstalledPlugin,
    ) -> Result<LoadedSource, EngineError> {
        let options = SecretsOptions {
            log: self
                .plugin_execution_log()
                .unwrap_or_else(|| Arc::new(ExecutionLog::new())),
            health: self.plugin_health(&package.manifest.id).await,
            settings: stored_plugin_settings(
                self.db().ok(),
                &package.manifest.id,
                &package.manifest.settings,
            )
            .await,
            refused_ports: match MCP_PORT.load(Ordering::Relaxed) {
                0 => Vec::new(),
                port => vec![port],
            },
        };
        let name = package.manifest.name.clone();
        let built = tokio::task::spawn_blocking(move || {
            SecretsPlugin::new(&package.manifest, &package.component, options)
        })
        .await;
        match built {
            Ok(Ok(plugin)) => Ok(LoadedSource {
                name,
                backend: Backend::Plugin(plugin),
            }),
            Ok(Err(failure)) => Err(SecretSourceError::Failed {
                name,
                code: failure.code().unwrap_or("plugin_failed").to_owned(),
            }
            .into()),
            Err(error) => {
                tracing::warn!(%error, "a secret source could not be built");
                Err(SecretSourceError::Failed {
                    name,
                    code: "plugin_failed".to_owned(),
                }
                .into())
            }
        }
    }
}

impl SecretSourceError {
    /// Why a call to the source `name` failed with `kind` and `code`, by the `state` its
    /// `status` answered afterwards: a source that says it is ready lacks the entry, or failed by
    /// itself.
    fn from_state(
        name: String,
        state: Option<SourceState>,
        kind: FailureKind,
        code: String,
    ) -> Self {
        match state {
            Some(SourceState::Locked) => Self::Locked { name },
            Some(SourceState::Unreachable) => Self::Unreachable { name },
            Some(SourceState::Unlicensed) => Self::Unlicensed { name },
            Some(SourceState::Ready) | None if kind == FailureKind::NotFound => {
                Self::EntryMissing { name }
            }
            Some(SourceState::Ready) | None => Self::Failed { name, code },
        }
    }
}

impl From<SecretSourceError> for EngineError {
    fn from(error: SecretSourceError) -> Self {
        Self::SecretSource(error)
    }
}

#[cfg(test)]
#[path = "secret_sources_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "secret_sources_wincred_tests.rs"]
mod wincred_seam_tests;
