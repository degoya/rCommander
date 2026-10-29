//! Each plugin with its versions, as `plugins.list` and the Plugins page show it: built from the
//! host's inventory of the plugin folder, the person's rows in the database and each plugin's
//! health.

use std::collections::BTreeMap;

use rc_plugin_host::{
    Permissions, PluginManifest, PluginState, SettingKind, VersionEntry, VersionRole, installed,
};

use super::{PluginAdminError, PluginInfo, PluginSettingInfo, VersionInfo};
use crate::{Engine, EngineError};

/// The rCommander setting of a plugin's own setting `name`.
pub(crate) fn setting_key(id: &str, name: &str) -> String {
    format!("plugin.{id}.{name}")
}

impl Engine {
    /// Every installed version under `state`, read off the UI thread.
    pub(super) async fn plugin_inventory(
        &self,
        state: &PluginState,
    ) -> Result<Vec<VersionEntry>, EngineError> {
        let data_dir = self.plugin_desk()?.data_dir().to_path_buf();
        let state = state.clone();
        Ok(
            tokio::task::spawn_blocking(move || installed::inventory(&data_dir, &state))
                .await
                .map_err(PluginAdminError::from)?
                .map_err(|error| PluginAdminError::from_host(&error))?,
        )
    }

    /// The plugins under `state`, by name; only plugin `only` when given.
    pub(super) async fn plugin_infos(
        &self,
        state: &PluginState,
        only: Option<&str>,
    ) -> Result<Vec<PluginInfo>, EngineError> {
        let mut by_id: BTreeMap<String, Vec<VersionEntry>> = BTreeMap::new();
        for entry in self.plugin_inventory(state).await? {
            if only.is_none_or(|id| id == entry.id) {
                by_id.entry(entry.id.clone()).or_default().push(entry);
            }
        }
        let db = self.db()?;
        let mut plugins = Vec::with_capacity(by_id.len());
        for (id, entries) in by_id {
            let row = db.plugin(&id).await?;
            let pending = row.as_ref().and_then(|row| row.pending_version.clone());
            let running = entries
                .iter()
                .find(|entry| entry.role == Some(VersionRole::Default));
            // What the plugin is: the version that runs, else the newest that verifies.
            let shown = running.or_else(|| entries.iter().find(|entry| entry.manifest.is_some()));
            let manifest = shown.and_then(|entry| entry.manifest.as_ref());
            let mut settings = Vec::new();
            for declared in manifest.map_or(&[][..], |manifest| &manifest.settings[..]) {
                let setting = setting_key(&id, &declared.key);
                let value = db.get::<String>(&setting).await.ok().flatten();
                // A masked value never leaves through MCP; the window shows it hidden.
                let hidden = declared.masked && crate::read_guard::from_mcp();
                settings.push(PluginSettingInfo {
                    value: value.map(|value| {
                        if hidden {
                            crate::REDACTED.to_owned()
                        } else {
                            value
                        }
                    }),
                    masked: declared.masked,
                    setting,
                    name: declared.key.clone(),
                    kind: match declared.kind {
                        SettingKind::Choice => "choice",
                        SettingKind::Text => "text",
                    }
                    .to_owned(),
                    values: declared.values.clone(),
                    multiline: declared.multiline,
                    default: declared.default.clone(),
                });
            }
            plugins.push(PluginInfo {
                name: manifest.map_or_else(|| id.clone(), |manifest| manifest.name.clone()),
                kind: text(manifest, |manifest| {
                    manifest.plugin_type.as_str().to_owned()
                }),
                description: text(manifest, |manifest| manifest.metadata.description.clone()),
                author: text(manifest, |manifest| manifest.metadata.author.clone()),
                version: running.map(|entry| entry.version.clone()),
                enabled: state.enabled(&id),
                suspended: self.plugin_suspended(&id),
                bundled: shown.is_some_and(|entry| entry.bundled),
                signed: shown.is_some_and(|entry| entry.signed),
                key_id: text(manifest, |manifest| manifest.key_id.clone()),
                key_fingerprint: shown.and_then(|entry| entry.fingerprint.clone()),
                permissions: manifest.map(Permissions::of).unwrap_or_default(),
                versions: entries
                    .iter()
                    .map(|entry| version_info(entry, pending.as_deref()))
                    .collect(),
                settings,
                id,
            });
        }
        plugins.sort_by_key(|plugin| plugin.name.to_lowercase());
        Ok(plugins)
    }

    /// Whether plugin `id` is suspended; never creates its health.
    fn plugin_suspended(&self, id: &str) -> bool {
        self.inner.plugins.get().is_some_and(|desk| {
            desk.health
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(id)
                .is_some_and(|health| health.is_suspended())
        })
    }
}

fn text(manifest: Option<&PluginManifest>, field: impl Fn(&PluginManifest) -> String) -> String {
    manifest.map(field).unwrap_or_default()
}

fn version_info(entry: &VersionEntry, pending: Option<&str>) -> VersionInfo {
    let state = match (entry.refused, entry.role) {
        (Some(_), _) | (None, None) => "refused",
        _ if pending == Some(entry.version.as_str()) => "pending",
        (None, Some(VersionRole::Default)) => "active",
        (None, Some(VersionRole::Staged)) => "staged",
        (None, Some(VersionRole::Retained)) => "previous",
    };
    VersionInfo {
        version: entry.version.clone(),
        state: state.to_owned(),
        refused: entry.refused.map(str::to_owned),
        bundled: entry.bundled,
        signed: entry.signed,
        in_use: entry.in_use,
    }
}
