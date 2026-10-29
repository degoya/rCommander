//! The installed plugins as the engine keeps them (RC-100-09, `docs/plugins.md` "Installation
//! and lifecycle"): the plugin folder under the data folder, the person's decisions in the
//! database, the state file `rc_plugin_host::installed::list` reads, the execution log written
//! through into `plugin_executions`, and each plugin's health.
//!
//! The database is the record; [`Engine::sync_plugin_state`] writes `plugins/state.json` from it
//! after every change, so the viewer adapter (RC-100-13), which lists the plugins with nothing but
//! the data folder, verifies against the same keys, withdrawals, switches and versions; every
//! change is announced as `EngineEvent::PluginsChanged`. The
//! `plugins.*` commands are in `plugin_admin/commands.rs`; all but `plugins.list` are `GuiOnly`:
//! installing, switching on or rolling back decides which foreign code runs.

mod commands;
mod error;
mod info;
mod listing;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use rc_db::{Db, NewPluginExecution};
use rc_plugin_host::{
    ExecutionLog, ExecutionRecord, PluginHealth, PluginState, VersionChoice, installed,
};
use serde_json::Value;

pub use error::PluginAdminError;
pub(crate) use listing::setting_key;
// What a preview and a listing carry, for the window.
pub use info::{PluginInfo, PluginSettingInfo, PluginsOutcome, VersionInfo};
pub use rc_plugin_host::{
    KeyStatus, PackagePreview, Permissions, installed::bundled_dir as bundled_plugins_dir,
};

use crate::{
    Engine, EngineError, EngineEvent,
    settings::{CRASH_LIMIT_KEY, DEVELOPMENT_MODE_KEY, PLUGINS_ENABLED_KEY, stored_setting},
};

/// How many execution records wait for the database at most; past it a record is dropped, not
/// queued without end while the database is slow.
const EXECUTION_QUEUE: usize = 1024;

/// Where the plugins live and what is known about their calls.
pub(crate) struct PluginDesk {
    /// The data folder; the packages are in its `plugins/`.
    data_dir: PathBuf,
    /// `plugins/` beside the executable, with the bundled packages.
    bundled: Option<PathBuf>,
    log: Arc<ExecutionLog>,
    health: Mutex<HashMap<String, Arc<PluginHealth>>>,
}

impl PluginDesk {
    pub(crate) fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

/// What the start did with the installed plugins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginStart {
    /// `(id, version)` of each bundled package installed now.
    pub bundled: Vec<(String, String)>,
    /// The start's sweep: leftovers removed, outdated versions removed, broken ones kept.
    pub sweep: installed::StartSweep,
}

impl Engine {
    /// Gives the engine its plugin folder `<data_dir>/plugins` and the bundled packages in
    /// `bundled`; `false` when it has one already. With a database attached, every plugin call
    /// is written into `plugin_executions` from now on.
    pub fn attach_plugins(&self, data_dir: PathBuf, bundled: Option<PathBuf>) -> bool {
        let log = Arc::new(ExecutionLog::new());
        if let (Ok(db), Ok(runtime)) = (self.db(), tokio::runtime::Handle::try_current()) {
            let (sender, receiver) = tokio::sync::mpsc::channel(EXECUTION_QUEUE);
            log.set_sink(Arc::new(move |record: &ExecutionRecord| {
                // Full: the database lags behind, and the record is only missing from the table
                // (the in-memory log still has it). Closed: the app ends.
                if sender.try_send(execution(record)).is_err() {
                    tracing::debug!("a plugin call was not queued for the execution table");
                }
            }));
            runtime.spawn(write_executions(db.clone(), receiver));
        }
        let desk = PluginDesk {
            data_dir,
            bundled,
            log,
            health: Mutex::default(),
        };
        self.inner.plugins.set(desk).is_ok()
    }

    /// The data folder whose `plugins/` holds the installed plugins - what
    /// `rc_plugin_host::installed::list` takes; `None` before [`Engine::attach_plugins`].
    #[must_use]
    pub fn plugins_data_dir(&self) -> Option<&Path> {
        self.inner.plugins.get().map(PluginDesk::data_dir)
    }

    /// The execution log every plugin call is recorded in (a viewer's `ViewerOptions::log`).
    #[must_use]
    pub fn plugin_execution_log(&self) -> Option<Arc<ExecutionLog>> {
        self.inner.plugins.get().map(|desk| Arc::clone(&desk.log))
    }

    /// The health of plugin `id`, shared by all its versions and sessions, held to the current
    /// `plugins.crash_limit` (a viewer's `ViewerOptions::health`).
    pub async fn plugin_health(&self, id: &str) -> Arc<PluginHealth> {
        let limit = self.crash_limit().await;
        let health = match self.inner.plugins.get() {
            Some(desk) => Arc::clone(
                desk.health
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .entry(id.to_owned())
                    .or_insert_with(|| Arc::new(PluginHealth::new(limit))),
            ),
            None => Arc::new(PluginHealth::new(limit)),
        };
        health.set_crash_limit(limit);
        health
    }

    /// The start's work on the plugins, before anything runs one: an update installed last run
    /// takes over, the bundled packages not installed yet are installed, the start's sweep runs
    /// and the state file is written.
    ///
    /// # Errors
    ///
    /// No plugin folder or database, or the state cannot be written.
    pub async fn start_plugins(&self) -> Result<PluginStart, EngineError> {
        let desk = self.plugin_desk()?;
        let db = self.db()?;
        for row in db.plugins().await? {
            if row.pending_version.is_some() {
                db.set_plugin_versions(&row.id, None, row.staged_version.as_deref(), None)
                    .await?;
            }
        }
        let state = self.sync_plugin_state().await?;
        let mut start = PluginStart::default();
        if let Some(bundled) = desk.bundled.clone() {
            let data_dir = desk.data_dir.clone();
            start.bundled =
                tokio::task::spawn_blocking(move || installed::sync_bundled(&data_dir, &bundled))
                    .await
                    .map_err(PluginAdminError::from)?
                    .map_err(|error| PluginAdminError::from_host(&error))?;
            for (id, _) in &start.bundled {
                db.record_plugin(id, true).await?;
            }
        }
        let data_dir = desk.data_dir.clone();
        start.sweep =
            tokio::task::spawn_blocking(move || installed::sweep_at_start(&data_dir, &state))
                .await
                .map_err(PluginAdminError::from)?;
        self.publish(EngineEvent::PluginsChanged);
        Ok(start)
    }

    /// The plugins' state as the database and the settings hold it.
    pub(crate) async fn plugin_state(&self) -> Result<PluginState, EngineError> {
        let db = self.db()?;
        let mut state = PluginState {
            plugins_enabled: self.setting_on(PLUGINS_ENABLED_KEY).await,
            development_mode: self.setting_on(DEVELOPMENT_MODE_KEY).await,
            ..PluginState::default()
        };
        for row in db.plugins().await? {
            if !row.enabled {
                state.disabled.insert(row.id.clone());
            }
            if row.active_version.is_some() || row.staged_version.is_some() {
                state.choices.insert(
                    row.id,
                    VersionChoice {
                        active: row.active_version,
                        staged: row.staged_version,
                    },
                );
            }
        }
        for key in db.plugin_keys().await? {
            if key.revoked_at.is_some() {
                state.revoked_keys.insert(key.fingerprint);
            } else {
                state.trusted_keys.insert(key.key_id, key.public_key);
            }
        }
        state.revoked_digests = db.plugin_digest_revocations().await?.into_iter().collect();
        Ok(state)
    }

    /// Writes the state file from the database; the state written.
    ///
    /// # Errors
    ///
    /// No plugin folder or database, or the file cannot be written.
    pub async fn sync_plugin_state(&self) -> Result<PluginState, EngineError> {
        let desk = self.plugin_desk()?;
        let state = self.plugin_state().await?;
        let (written, data_dir) = (state.clone(), desk.data_dir.clone());
        tokio::task::spawn_blocking(move || written.write(&data_dir))
            .await
            .map_err(PluginAdminError::from)?
            .map_err(|error| PluginAdminError::from_host(&error))?;
        Ok(state)
    }

    /// A `plugins.*` setting changed: the state file follows at once, and so does every
    /// plugin's crash limit. Without a plugin folder there is nothing to follow.
    pub(crate) async fn plugin_settings_changed(&self) {
        let Some(desk) = self.inner.plugins.get() else {
            return;
        };
        if let Err(error) = self.sync_plugin_state().await {
            tracing::warn!(code = error.code(), "the plugin state was not written");
        }
        let limit = self.crash_limit().await;
        for health in desk
            .health
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            health.set_crash_limit(limit);
        }
    }

    async fn crash_limit(&self) -> u32 {
        let stored = match self.db() {
            Ok(db) => stored_setting(db, CRASH_LIMIT_KEY).await,
            Err(_) => None,
        };
        stored
            .as_ref()
            .and_then(Value::as_u64)
            .and_then(|limit| u32::try_from(limit).ok())
            .unwrap_or(rc_plugin_host::DEFAULT_CRASH_LIMIT)
    }

    pub(crate) fn plugin_desk(&self) -> Result<&PluginDesk, PluginAdminError> {
        self.inner
            .plugins
            .get()
            .ok_or(PluginAdminError::Unavailable)
    }
}

/// A record as `plugin_executions` stores it.
fn execution(record: &ExecutionRecord) -> NewPluginExecution {
    NewPluginExecution {
        plugin_id: record.plugin_id.clone(),
        plugin_version: record.plugin_version.clone(),
        plugin_type: record.plugin_type.to_owned(),
        operation: record.operation.to_owned(),
        outcome: record.outcome.as_str().to_owned(),
        code: record.code.clone(),
        duration_ms: i64::try_from(record.duration.as_millis()).unwrap_or(i64::MAX),
    }
}

/// Writes each record into the database as it comes; a record that cannot be written is only
/// missing from the table, never a failure of the call.
async fn write_executions(db: Db, mut records: tokio::sync::mpsc::Receiver<NewPluginExecution>) {
    while let Some(record) = records.recv().await {
        if let Err(error) = db.record_plugin_execution(&record).await {
            tracing::debug!(%error, "a plugin call was not recorded");
        }
    }
}

impl From<PluginAdminError> for EngineError {
    fn from(error: PluginAdminError) -> Self {
        Self::Plugin(error)
    }
}

#[cfg(test)]
#[path = "plugin_admin_tests.rs"]
mod tests;
