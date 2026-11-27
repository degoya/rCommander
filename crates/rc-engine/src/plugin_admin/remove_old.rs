//! `plugins.remove_old` (RC-110-55): every old version of every plugin at once - "Remove old
//! versions..." on the Plugins page, after the person saw the list; `GuiOnly`, as
//! `plugins.remove`.
//!
//! Old is a version that does not verify, and one that verifies but does not run: every
//! `Retained` version, older or newer than the one that runs, a rollback's target included (the
//! owner, 2026-11-27). Kept are the version that runs (`Default`), a staged one, one that runs
//! from the next start (`pending`) and a bundled version the shipped packages still hold, which
//! the next start's `sync_bundled` would install again. An old version an open viewer holds is named
//! apart and left. Each version goes the way `plugins.remove` takes it - the folder, then the
//! person's rows - and the state file and one `PluginsChanged` follow at the end.

use std::collections::BTreeSet;

use rc_plugin_host::{PluginInstaller, PluginState, VersionEntry, VersionRole, installed};

use super::{OldReason, OldVersion, PluginAdminError, PluginVersionRef, PluginsOutcome};
use crate::recover::LogPanic as _;
use crate::{Engine, EngineError, EngineEvent, Outcome};

/// The old versions, and those of them an open viewer holds.
type OldAndHeld = (Vec<OldVersion>, Vec<OldVersion>);

impl Engine {
    /// `plugins.remove_old`: without `confirmed`, the old versions and the held ones; with it,
    /// removes each confirmed version that is still old now - one an open viewer holds is
    /// skipped and named, one gone meanwhile is passed over.
    pub(crate) async fn remove_old_plugin_versions(
        &self,
        confirmed: Option<&[PluginVersionRef]>,
    ) -> Result<Outcome, EngineError> {
        self.plugin_desk()?;
        let (old, held) = self.old_plugin_versions().await?;
        let Some(confirmed) = confirmed else {
            return Ok(answer(PluginsOutcome::OldVersions {
                versions: old,
                held,
            }));
        };
        let mut skipped: Vec<OldVersion> = held
            .into_iter()
            .filter(|held| named_in(confirmed, held))
            .collect();
        let mut removed = Vec::new();
        let mut failed = None;
        let state = self.plugin_state().await?;
        for version in old.into_iter().filter(|old| named_in(confirmed, old)) {
            match self
                .remove_version_quietly(&state, &version.id, &version.version)
                .await
            {
                Ok(true) => removed.push(version),
                Ok(false) => {}
                // Held since the list was read: left, and named.
                Err(error) if error.code() == "plugin_version_in_use" => skipped.push(version),
                Err(error) => {
                    failed = Some(error);
                    break;
                }
            }
        }
        // What went before a failure is gone: the state file and the window follow it.
        self.sync_plugin_state().await?;
        self.publish(EngineEvent::PluginsChanged);
        if let Some(error) = failed {
            return Err(error);
        }
        Ok(answer(PluginsOutcome::OldRemoved { removed, skipped }))
    }

    /// Removes version `version` of plugin `id` from the plugin folder and from the person's
    /// rows - a plugin whose last version goes is forgotten; `false` when it is not installed.
    /// The state file and the event are the caller's. Refused while an open viewer holds it
    /// (`plugin_version_in_use`).
    pub(super) async fn remove_version_quietly(
        &self,
        state: &PluginState,
        id: &str,
        version: &str,
    ) -> Result<bool, EngineError> {
        let desk = self.plugin_desk()?;
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
        .log_panic("engine.plugin_remove")
        .map_err(PluginAdminError::from)?
        .map_err(|error| PluginAdminError::from_host(&error))?;
        if !removed {
            return Ok(false);
        }
        let db = self.db()?;
        let left = self.plugin_infos(state, Some(id)).await?;
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
        Ok(true)
    }

    /// The old versions under the current state, and those of them an open viewer holds.
    async fn old_plugin_versions(&self) -> Result<OldAndHeld, EngineError> {
        let state = self.plugin_state().await?;
        let entries = self.plugin_inventory(&state).await?;
        let mut pending = BTreeSet::new();
        for row in self.db()?.plugins().await? {
            if let Some(version) = row.pending_version {
                pending.insert((row.id, version));
            }
        }
        let candidates = old_entries(&entries, &pending);
        // The shipped packages are read only when a bundled version is among them.
        let shipped = if candidates.iter().any(|(entry, _)| entry.bundled) {
            self.shipped_plugin_versions().await?
        } else {
            BTreeSet::new()
        };
        let (mut old, mut held) = (Vec::new(), Vec::new());
        for (entry, reason) in candidates {
            if entry.bundled && shipped.contains(&(entry.id.clone(), entry.version.clone())) {
                continue;
            }
            let version = old_version(entry, reason);
            if entry.in_use {
                held.push(version);
            } else {
                old.push(version);
            }
        }
        // By name, each plugin's versions newest first as the inventory has them.
        old.sort_by_key(|version| version.name.to_lowercase());
        held.sort_by_key(|version| version.name.to_lowercase());
        Ok((old, held))
    }

    /// `(id, version)` of every package in the bundled packages' folder, read off the UI thread;
    /// none without that folder.
    async fn shipped_plugin_versions(&self) -> Result<BTreeSet<(String, String)>, EngineError> {
        let Some(bundled) = self.plugin_desk()?.bundled.clone() else {
            return Ok(BTreeSet::new());
        };
        Ok(
            tokio::task::spawn_blocking(move || installed::bundled_versions(&bundled))
                .await
                .log_panic("engine.plugin_bundled_versions")
                .map_err(PluginAdminError::from)?,
        )
    }
}

/// Whether the person confirmed `version`.
fn named_in(confirmed: &[PluginVersionRef], version: &OldVersion) -> bool {
    confirmed
        .iter()
        .any(|named| named.id == version.id && named.version == version.version)
}

fn answer(outcome: PluginsOutcome) -> Outcome {
    Outcome::Plugins(Box::new(outcome))
}

/// Every old entry of `entries` with why it is old; `pending` holds the `(id, version)` that
/// run from the next start. The bundled and held ones are the caller's.
fn old_entries<'a>(
    entries: &'a [VersionEntry],
    pending: &BTreeSet<(String, String)>,
) -> Vec<(&'a VersionEntry, OldReason)> {
    entries
        .iter()
        .filter_map(|entry| {
            let reason = if entry.refused.is_some() || entry.manifest.is_none() {
                OldReason::Refused
            } else if entry.role == Some(VersionRole::Retained)
                && !pending.contains(&(entry.id.clone(), entry.version.clone()))
            {
                OldReason::Inactive
            } else {
                return None;
            };
            Some((entry, reason))
        })
        .collect()
}

fn old_version(entry: &VersionEntry, reason: OldReason) -> OldVersion {
    let name = entry
        .manifest
        .as_ref()
        .map(|manifest| manifest.name.clone())
        .or_else(|| entry.label.as_ref().map(|label| label.name.clone()))
        .unwrap_or_else(|| entry.id.clone());
    OldVersion {
        id: entry.id.clone(),
        name,
        version: entry.version.clone(),
        reason,
        refused: entry.refused.map(str::to_owned),
    }
}

#[cfg(test)]
#[path = "remove_old_tests.rs"]
mod tests;
