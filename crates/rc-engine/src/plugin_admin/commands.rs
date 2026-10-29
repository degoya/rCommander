//! The `plugins.*` commands: list, preview, install, enable, disable, roll back, resume, remove -
//! and a plugin's own settings, which only the window sets (no command).
//!
//! Copied in its flow from rDownloader's plugin manager (`install.rs`, `preview.rs`,
//! `versions.rs`, RD-140-02, RD-170-12) at commit 26c8592a6: a first install runs at once, an
//! update from the next start; an unknown key is confirmed by its fingerprint from the preview,
//! never by a separate step a caller could skip; the person's decisions go into the database and
//! the state file follows.

use std::path::{Path, PathBuf};

use rc_plugin_host::{
    MAX_PACKAGE_BYTES, PluginInstaller, PluginState, SettingKind, SettingManifest, VerifiedPackage,
    VerifyError,
    installed::{self, InstalledKeys},
    preview_package,
};

use super::{PluginAdminError, PluginInfo, PluginsOutcome, listing::setting_key};
use crate::{Engine, EngineError, EngineEvent, Outcome};

/// How many of a plugin's latest calls `plugins.list` answers with.
const EXECUTIONS_SHOWN: u32 = 50;

fn answer(outcome: PluginsOutcome) -> Outcome {
    Outcome::Plugins(Box::new(outcome))
}

impl Engine {
    /// `plugins.list`: every installed plugin, or the one `id` names with its latest calls.
    pub(crate) async fn list_plugins(&self, id: Option<&str>) -> Result<Outcome, EngineError> {
        let state = self.plugin_state().await?;
        let plugins = self.plugin_infos(&state, id).await?;
        let executions = match id {
            Some(id) if !plugins.is_empty() => {
                self.db()?.plugin_executions(id, EXECUTIONS_SHOWN).await?
            }
            Some(id) => {
                return Err(PluginAdminError::NotFound { id: id.to_owned() }.into());
            }
            None => Vec::new(),
        };
        Ok(answer(PluginsOutcome::List {
            plugins,
            executions,
            plugins_enabled: state.plugins_enabled,
            development_mode: state.development_mode,
        }))
    }

    /// `plugins.preview`: what the package at `path` is, before anything is installed.
    pub(crate) async fn preview_plugin(&self, path: &str) -> Result<Outcome, EngineError> {
        let state = self.plugin_state().await?;
        let installed_keys = self.installed_keys().await?;
        let verifier = state
            .verifier()
            .map_err(|error| PluginAdminError::from_host(&error))?
            .with_installed_keys(installed_keys);
        let bytes = read_package(PathBuf::from(path)).await?;
        let digest = rc_plugin_host::sha256_hex(&bytes);
        let preview =
            preview_package(&bytes, &verifier).map_err(|error| PluginAdminError::Refused {
                code: "plugin_invalid",
                reason: format!("{error:#}"),
            })?;
        let current = self.plugin_infos(&state, Some(&preview.id)).await?;
        let installed_versions = current
            .iter()
            .flat_map(|plugin| {
                plugin
                    .versions
                    .iter()
                    .map(|version| version.version.clone())
            })
            .collect();
        let added_permissions = current
            .first()
            .map(|plugin| plugin.permissions.added_by(&preview.permissions))
            .unwrap_or_default();
        Ok(answer(PluginsOutcome::Preview {
            path: path.to_owned(),
            digest,
            installable: preview.installable(state.development_mode),
            preview,
            installed_versions,
            added_permissions,
        }))
    }

    /// `plugins.install`: verifies and installs the package at `path`. A key nobody confirmed is
    /// trusted only when `fingerprint` is the one the preview showed for it; a plugin installed
    /// under another key is refused before that (`plugin_key_changed`, RC-080-43). The file must
    /// still be the one the preview read: `digest` is its SHA-256 from the preview, and a package
    /// swapped since - under the same key, with other permissions - is refused
    /// (`plugin_package_changed`, RC-080-53).
    pub(crate) async fn install_plugin(
        &self,
        path: &str,
        fingerprint: Option<&str>,
        digest: &str,
    ) -> Result<Outcome, EngineError> {
        let desk = self.plugin_desk()?;
        let state = self.plugin_state().await?;
        let installed_keys = self.installed_keys().await?;
        let bytes = read_package(PathBuf::from(path)).await?;
        if !rc_plugin_host::sha256_hex(&bytes).eq_ignore_ascii_case(digest) {
            return Err(PluginAdminError::Refused {
                code: "plugin_package_changed",
                reason: "the package file changed since its preview; preview it again".to_owned(),
            }
            .into());
        }
        let package = match verify(&state, &installed_keys, bytes.clone()).await? {
            Ok(package) => package,
            Err(VerifyError::UntrustedKey {
                key_id,
                public_key,
                fingerprint: shown,
                ..
            }) if fingerprint.is_some_and(|confirmed| confirmed.eq_ignore_ascii_case(&shown)) => {
                self.db()?
                    .trust_plugin_key(&key_id, &public_key, &shown)
                    .await?;
                let state = self.plugin_state().await?;
                verify(&state, &installed_keys, bytes)
                    .await?
                    .map_err(PluginAdminError::from_verify)?
            }
            Err(error) => return Err(PluginAdminError::from_verify(error).into()),
        };
        let id = package.manifest.id.clone();
        let version = package.manifest.version.clone();
        // A plugin with a version that runs gets the new one from the next start.
        let running = self
            .plugin_infos(&state, Some(&id))
            .await?
            .first()
            .and_then(|plugin| plugin.version.clone());
        let installer = PluginInstaller::new(
            installed::plugins_root(desk.data_dir()),
            state
                .verifier()
                .map_err(|error| PluginAdminError::from_host(&error))?,
        );
        tokio::task::spawn_blocking(move || installer.install_verified(&package, false))
            .await
            .map_err(PluginAdminError::from)?
            .map_err(|error| PluginAdminError::from_host(&error))?;
        let db = self.db()?;
        let row = db.record_plugin(&id, false).await?;
        if let Some(running) = &running {
            db.set_plugin_versions(
                &id,
                Some(running),
                row.staged_version.as_deref(),
                Some(&version),
            )
            .await?;
        }
        let plugin = self.changed_plugin(&id).await?;
        Ok(answer(PluginsOutcome::Installed {
            plugin,
            version,
            runs_now: running.is_none(),
        }))
    }

    /// `plugins.enable` and `plugins.disable`.
    pub(crate) async fn enable_plugin(&self, id: &str, on: bool) -> Result<Outcome, EngineError> {
        self.known_plugin(id).await?;
        let db = self.db()?;
        db.record_plugin(id, false).await?;
        db.set_plugin_enabled(id, on).await?;
        Ok(answer(PluginsOutcome::Changed {
            plugin: self.changed_plugin(id).await?,
        }))
    }

    /// `plugins.rollback`: the newest version older than the one that runs runs instead.
    pub(crate) async fn rollback_plugin(&self, id: &str) -> Result<Outcome, EngineError> {
        let plugin = self.known_plugin(id).await?;
        let loaded: Vec<&str> = plugin
            .versions
            .iter()
            .filter(|version| version.state != "refused")
            .map(|version| version.version.as_str())
            .collect();
        let previous = plugin
            .version
            .as_deref()
            .and_then(|current| rc_plugin_host::previous_version(&loaded, current))
            .ok_or_else(|| PluginAdminError::NoPreviousVersion { id: id.to_owned() })?;
        let db = self.db()?;
        let row = db.record_plugin(id, false).await?;
        db.set_plugin_versions(id, Some(previous), row.staged_version.as_deref(), None)
            .await?;
        Ok(answer(PluginsOutcome::Changed {
            plugin: self.changed_plugin(id).await?,
        }))
    }

    /// `plugins.resume`: a suspended plugin runs again, its failures counted from zero.
    pub(crate) async fn resume_plugin(&self, id: &str) -> Result<Outcome, EngineError> {
        self.known_plugin(id).await?;
        self.plugin_health(id).await.resume();
        Ok(answer(PluginsOutcome::Changed {
            plugin: self.changed_plugin(id).await?,
        }))
    }

    /// `plugins.remove`: one installed version; refused while an open viewer holds it.
    pub(crate) async fn remove_plugin_version(
        &self,
        id: &str,
        version: &str,
    ) -> Result<Outcome, EngineError> {
        let desk = self.plugin_desk()?;
        let state = self.plugin_state().await?;
        let installer = PluginInstaller::new(
            installed::plugins_root(desk.data_dir()),
            state
                .verifier()
                .map_err(|error| PluginAdminError::from_host(&error))?,
        );
        let (owned_id, owned_version) = (id.to_owned(), version.to_owned());
        let removed = tokio::task::spawn_blocking(move || {
            installer.remove_version(&owned_id, &owned_version)
        })
        .await
        .map_err(PluginAdminError::from)?
        .map_err(|error| PluginAdminError::from_host(&error))?;
        if !removed {
            return Err(PluginAdminError::NotFound { id: id.to_owned() }.into());
        }
        let db = self.db()?;
        let left = self.plugin_infos(&state, Some(id)).await?;
        match (db.plugin(id).await?, left.is_empty()) {
            (Some(_), true) => {
                db.remove_plugin(id).await?;
            }
            (Some(row), false) => {
                let keep = |chosen: Option<String>| chosen.filter(|chosen| chosen != version);
                db.set_plugin_versions(
                    id,
                    keep(row.active_version).as_deref(),
                    keep(row.staged_version).as_deref(),
                    keep(row.pending_version).as_deref(),
                )
                .await?;
            }
            (None, _) => {}
        }
        self.sync_plugin_state().await?;
        self.publish(EngineEvent::PluginsChanged);
        Ok(answer(PluginsOutcome::Removed {
            id: id.to_owned(),
            version: version.to_owned(),
        }))
    }

    /// Stores a plugin's own setting `name` - only a value its manifest's `[[settings]]` takes.
    /// The window's way; no command, so MCP cannot reach it (`plugin.*` is protected).
    ///
    /// # Errors
    ///
    /// The plugin is not installed, declares no such setting or refuses the value.
    pub async fn set_plugin_setting(
        &self,
        id: &str,
        name: &str,
        value: &str,
    ) -> Result<PluginInfo, EngineError> {
        let plugin = self.known_plugin(id).await?;
        let declared = plugin
            .settings
            .iter()
            .find(|setting| setting.name == name)
            .ok_or_else(|| PluginAdminError::SettingRefused {
                id: id.to_owned(),
                name: name.to_owned(),
            })?;
        // The host's own rule for the value, on the row the manifest declared.
        let rule = SettingManifest {
            key: declared.name.clone(),
            kind: if declared.kind == "choice" {
                SettingKind::Choice
            } else {
                SettingKind::Text
            },
            values: declared.values.clone(),
            multiline: declared.multiline,
            default: declared.default.clone(),
            masked: declared.masked,
        };
        let accepted = rule.accepts(value);
        if !accepted {
            return Err(PluginAdminError::SettingRefused {
                id: id.to_owned(),
                name: name.to_owned(),
            }
            .into());
        }
        self.db()?.set(&setting_key(id, name), value).await?;
        self.changed_plugin(id).await
    }

    /// The plugin `id` as listed now; `plugin_not_found` when it is not installed.
    async fn known_plugin(&self, id: &str) -> Result<PluginInfo, EngineError> {
        let state = self.plugin_state().await?;
        self.plugin_infos(&state, Some(id))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| PluginAdminError::NotFound { id: id.to_owned() }.into())
    }

    /// The signing keys the installed plugins were published under, read off the UI thread.
    async fn installed_keys(&self) -> Result<InstalledKeys, EngineError> {
        let data_dir = self.plugin_desk()?.data_dir().to_path_buf();
        Ok(
            tokio::task::spawn_blocking(move || InstalledKeys::read(&data_dir))
                .await
                .map_err(PluginAdminError::from)?
                .map_err(|error| PluginAdminError::from_host(&error))?,
        )
    }

    /// The state file written after a change, the change announced, and the plugin as it stands
    /// now.
    async fn changed_plugin(&self, id: &str) -> Result<PluginInfo, EngineError> {
        let state = self.sync_plugin_state().await?;
        self.publish(EngineEvent::PluginsChanged);
        self.plugin_infos(&state, Some(id))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| PluginAdminError::NotFound { id: id.to_owned() }.into())
    }
}

/// Reads a package file within the largest package's size, off the UI thread.
async fn read_package(path: PathBuf) -> Result<Vec<u8>, PluginAdminError> {
    tokio::task::spawn_blocking(move || read_bounded(&path))
        .await
        .map_err(PluginAdminError::from)?
}

/// A regular file only (`import::bounded_read`): a pipe named as a package would block a
/// thread of the blocking pool for good.
fn read_bounded(path: &Path) -> Result<Vec<u8>, PluginAdminError> {
    use crate::import::bounded_read::{self, BoundedReadError};

    let refused = |reason: String| PluginAdminError::Refused {
        code: "plugin_invalid",
        reason,
    };
    match bounded_read::read_bounded(path, MAX_PACKAGE_BYTES) {
        // A package holds no secret: its bytes leave the zeroizing buffer without a copy.
        Ok(mut bytes) => Ok(std::mem::take(&mut *bytes)),
        Err(BoundedReadError::TooLarge { .. }) => {
            Err(refused("the package is larger than any plugin".to_owned()))
        }
        Err(error) => Err(refused(format!("{}: {error}", path.display()))),
    }
}

/// Verifies `bytes` in full under `state` as a new package for the plugins `installed`, compile
/// included, off the UI thread: the outer error is the engine's, the inner the verification's
/// answer.
async fn verify(
    state: &PluginState,
    installed: &InstalledKeys,
    bytes: Vec<u8>,
) -> Result<Result<VerifiedPackage, VerifyError>, PluginAdminError> {
    let verifier = state
        .verifier()
        .map_err(|error| PluginAdminError::from_host(&error))?
        .with_installed_keys(installed.clone());
    Ok(tokio::task::spawn_blocking(move || verifier.verify_bytes(&bytes)).await?)
}
