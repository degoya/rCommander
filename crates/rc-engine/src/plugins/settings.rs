//! The `plugins.*` settings the viewer adapter reads itself; their rows are RC-100-09's
//! (`settings/schema/plugins.rs`). `plugins.crash_limit` reaches a plugin through its shared
//! health ([`crate::Engine::plugin_health`]); a plugin's own `plugin.<id>.<key>` reach it as
//! `ViewerOptions::settings` ([`stored_plugin_settings`], RC-100-14).

use std::collections::BTreeMap;

use rc_db::Db;
use rc_plugin_host::{DEFAULT_OUTPUT_LIMIT_BYTES, SettingManifest};

use crate::{
    plugin_admin::setting_key,
    settings::{
        ASSOCIATIONS_KEY, OUTPUT_LIMIT_KEY, PLUGINS_ENABLED_KEY, parse_associations, stored_setting,
    },
};

const MIB: u64 = 1024 * 1024;

/// What the settings decide about the viewer plugins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginSettings {
    /// `plugins.enabled`, the master switch: off, no plugin runs.
    pub enabled: bool,
    /// `plugins.viewer_output_limit_mib` in bytes: the most one output may write.
    pub output_limit: u64,
    /// `plugins.associations`: extension (lower case, without the dot) to the id of the plugin
    /// that shows it.
    pub associations: BTreeMap<String, String>,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            output_limit: DEFAULT_OUTPUT_LIMIT_BYTES,
            associations: BTreeMap::new(),
        }
    }
}

impl PluginSettings {
    /// The settings stored in `db`; the defaults without a database.
    pub async fn read(db: Option<&Db>) -> Self {
        let defaults = Self::default();
        let Some(db) = db else {
            return defaults;
        };
        let enabled = stored_setting(db, PLUGINS_ENABLED_KEY)
            .await
            .and_then(|value| value.as_bool())
            .unwrap_or(defaults.enabled);
        let output_limit = stored_setting(db, OUTPUT_LIMIT_KEY)
            .await
            .and_then(|value| value.as_u64())
            .filter(|mib| *mib > 0)
            .map_or(defaults.output_limit, |mib| mib.saturating_mul(MIB));
        let associations = stored_setting(db, ASSOCIATIONS_KEY)
            .await
            .and_then(|value| value.as_str().and_then(parse_associations))
            .map(|pairs| pairs.into_iter().collect())
            .unwrap_or_default();
        Self {
            enabled,
            output_limit,
            associations,
        }
    }
}

/// The values the person stored for plugin `id`'s `declared` settings, `(key, value)`: only the
/// rows `plugin.<id>.<key>` of keys its manifest declares, nothing of another plugin's or of
/// rCommander's own. The host checks each against the manifest and fills in the defaults; none
/// without a database.
pub(crate) async fn stored_plugin_settings(
    db: Option<&Db>,
    id: &str,
    declared: &[SettingManifest],
) -> Vec<(String, String)> {
    let Some(db) = db else {
        return Vec::new();
    };
    let mut stored = Vec::new();
    for setting in declared {
        if let Ok(Some(value)) = db.get::<String>(&setting_key(id, &setting.key)).await {
            stored.push((setting.key.clone(), value));
        }
    }
    stored
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
