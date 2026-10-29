//! The installed viewer plugins loaded: `rc_plugin_host::installed::list` (RC-100-09) answers
//! the running version of each plugin, verified again; every enabled viewer among them is
//! compiled on the blocking pool with the output limit of the settings, the engine's execution
//! log (written through to `plugin_executions`), the plugin's shared health, so a suspension
//! and the person's Resume on the Plugins page hold for the viewer too, and the plugin's own
//! settings as stored now (RC-100-14): a change announces `PluginsChanged`, and the window loads
//! the viewers again for the next document.

use std::{path::Path, sync::Arc};

use rc_db::Db;
use rc_plugin_host::{ExecutionLog, PluginType, ViewerOptions, ViewerPlugin, installed};

use super::{
    set::{InstalledViewer, ViewerPlugins},
    settings::{PluginSettings, stored_plugin_settings},
};
use crate::Engine;

impl ViewerPlugins {
    /// Every installed, enabled viewer plugin of `engine`'s data folder, with the settings in
    /// `db`; none while `plugins.enabled` is off or before the engine knows its plugins. A
    /// package that fails to build is logged and left out.
    pub async fn load(engine: &Engine, db: Option<&Db>) -> Self {
        let settings = PluginSettings::read(db).await;
        let Some(data_dir) = engine
            .plugins_data_dir()
            .filter(|_| settings.enabled)
            .map(Path::to_path_buf)
        else {
            return Self::none();
        };
        let log = engine
            .plugin_execution_log()
            .unwrap_or_else(|| Arc::new(ExecutionLog::new()));
        let packages = match tokio::task::spawn_blocking(move || installed::list(&data_dir)).await {
            Ok(Ok(packages)) => packages,
            Ok(Err(error)) => {
                tracing::warn!(error = %format!("{error:#}"), "the installed plugins could not be listed");
                return Self::none();
            }
            Err(error) => {
                tracing::warn!(%error, "the installed plugins could not be listed");
                return Self::none();
            }
        };
        let mut built = Vec::new();
        for package in packages {
            if !package.enabled || package.manifest.plugin_type != PluginType::Viewer {
                continue;
            }
            let options = ViewerOptions {
                output_limit: settings.output_limit,
                log: Arc::clone(&log),
                health: engine.plugin_health(&package.manifest.id).await,
                settings: stored_plugin_settings(
                    db,
                    &package.manifest.id,
                    &package.manifest.settings,
                )
                .await,
            };
            let compiled = tokio::task::spawn_blocking(move || {
                let plugin = ViewerPlugin::new(&package.manifest, &package.component, options);
                (package, plugin)
            })
            .await;
            match compiled {
                Ok((package, Ok(plugin))) => built.push(InstalledViewer::installed(
                    package.manifest,
                    plugin,
                    &package.version_dir,
                )),
                Ok((package, Err(failure))) => tracing::warn!(
                    plugin_id = %package.manifest.id,
                    code = failure.code().unwrap_or_default(),
                    "a viewer plugin could not be built and is left out"
                ),
                Err(error) => tracing::warn!(%error, "a viewer plugin could not be built"),
            }
        }
        Self::from_plugins(built, settings.associations, log)
    }
}
