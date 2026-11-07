//! The `inventory.*` commands: the sources and the installed inventory plugins, a sync (or its
//! preview), and a source set up, changed or removed by the person.

use std::sync::Arc;

use rc_db::inventory::{
    INVENTORY_SECRET_COLUMN, INVENTORY_SECRETS_TABLE, InventoryEntry, InventorySource,
    InventorySourceDraft, InventorySourceId, InventorySync, SyncStatus,
};
use rc_plugin_api::inventory::{InventoryHost, Protocol};
use rc_plugin_host::{
    ExecutionLog, InstalledPlugin, InventoryList, InventoryOptions, InventoryPlugin, PluginType,
    secret_slots,
};
use rc_vault::{FieldRef, SecretString, VaultError};

use super::{
    InventoryOutcome, InventoryPluginInfo, InventoryReport, InventorySecret, InventorySourceError,
    InventorySourceRequest,
};
use crate::recover::LogPanic as _;
use crate::{
    ConnectionKind, DefaultView, Engine, EngineError, EngineEvent, Outcome, TreeChange, TreeItem,
    plugins::{failure_code, stored_plugin_settings},
    secret_sources::refused_ports,
};

/// The most refusals a report names; the rest are only counted.
pub const MAX_REFUSALS_SHOWN: usize = 20;

fn answer(outcome: InventoryOutcome) -> Outcome {
    Outcome::Inventory(Box::new(outcome))
}

impl Engine {
    /// `inventory.sources`: every source, and the inventory plugins one can be set up with.
    pub(crate) async fn list_inventory_sources(&self) -> Result<Outcome, EngineError> {
        let sources = self.db()?.inventory_sources().await?;
        let plugins = self
            .installed_of(PluginType::Inventory)
            .await
            .into_iter()
            .map(|package| InventoryPluginInfo {
                slots: secret_slots(&package.manifest).unwrap_or_default(),
                id: package.manifest.id,
                name: package.manifest.name,
                version: package.manifest.version,
            })
            .collect();
        Ok(answer(InventoryOutcome::Sources { sources, plugins }))
    }

    /// `inventory.refresh`: asks the source's plugin for its hosts and syncs the folder with
    /// them; with `preview` only the plan. A failure is recorded with the source.
    pub(crate) async fn refresh_inventory(
        &self,
        id: InventorySourceId,
        preview: bool,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let source = db.inventory_source(id).await?;
        let listed = match self.list_inventory(&source).await {
            Ok(listed) => listed,
            Err(error) => {
                if !preview {
                    db.record_inventory_outcome(id, error.code()).await?;
                }
                return Err(error);
            }
        };
        let entries: Vec<InventoryEntry> = listed.hosts.iter().map(entry).collect();
        let done = db
            .sync_inventory(id, &entries, &listed.skipped_ids, preview)
            .await?;
        let report = report(id, preview, &listed, &entries, &done);
        tracing::debug!(
            source = %id,
            preview,
            created = report.created,
            updated = report.updated,
            missing = report.missing,
            skipped = report.skipped,
            "inventory synced"
        );
        if !preview
            && (report.created + report.updated + report.missing > 0)
            && let Some(folder) = done.folder
        {
            self.publish(EngineEvent::ConnectionTree {
                item: TreeItem::Folder(folder),
                change: TreeChange::Updated,
            });
        }
        Ok(answer(InventoryOutcome::Synced(report)))
    }

    /// `inventory.source_save`: a source added or changed, and the values given for its slots
    /// sealed in the vault. A changed address or plugin drops the values stored before.
    pub(crate) async fn save_inventory_source(
        &self,
        request: InventorySourceRequest,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?;
        let draft = InventorySourceDraft {
            name: request.name,
            plugin_id: request.plugin_id,
            address: request.address,
            folder_id: request.folder_id,
        };
        let id = match request.id {
            Some(id) => {
                db.update_inventory_source(id, &draft).await?;
                id
            }
            None => db.create_inventory_source(&draft).await?,
        };
        for InventorySecret { slot, value } in &request.secrets {
            if value.is_empty() {
                continue;
            }
            self.store_inventory_secret(id, slot, value.secret())
                .await?;
        }
        let source = db.inventory_source(id).await?;
        Ok(answer(InventoryOutcome::Saved { source }))
    }

    /// `inventory.source_delete`: the source and its secrets go; its connections stay.
    pub(crate) async fn delete_inventory_source(
        &self,
        id: InventorySourceId,
    ) -> Result<Outcome, EngineError> {
        {
            let _writes = self.inner.secret_writes.lock().await;
            self.db()?.delete_inventory_source(id).await?;
        }
        self.announce_if_vault_unused().await;
        Ok(answer(InventoryOutcome::Deleted { source: id }))
    }

    /// `secret` sealed as `slot` of source `id`.
    async fn store_inventory_secret(
        &self,
        id: InventorySourceId,
        slot: &str,
        secret: &SecretString,
    ) -> Result<(), EngineError> {
        let _writes = self.inner.secret_writes.lock().await;
        let vault = self.vault_handle().await?;
        self.db()?
            .store_inventory_secret(id, slot, |row| {
                let field = FieldRef::new(INVENTORY_SECRETS_TABLE, INVENTORY_SECRET_COLUMN, row);
                vault.encrypt_str(&field, secret)
            })
            .await
            .map_err(|error| self.secret_write_failed(error.into()))??;
        self.remember_secret(secret);
        tracing::debug!(source = %id, slot, "inventory secret stored");
        Ok(())
    }

    /// The hosts the source's plugin lists now, checked by the host.
    async fn list_inventory(&self, source: &InventorySource) -> Result<InventoryList, EngineError> {
        let package = self
            .installed_of(PluginType::Inventory)
            .await
            .into_iter()
            .find(|package| package.manifest.id == source.plugin_id)
            .ok_or_else(|| InventorySourceError::PluginMissing {
                name: source.name.clone(),
            })?;
        let plugin = self.build_inventory(source, package).await?;
        Box::pin(plugin.list()).await.map_err(|failure| {
            InventorySourceError::Failed {
                name: source.name.clone(),
                code: failure_code(failure.code()),
            }
            .into()
        })
    }

    /// `package` compiled as the inventory of `source`: its address, the values of its slots
    /// (decrypted now and remembered for the MCP mask), the engine's log, health and settings.
    async fn build_inventory(
        &self,
        source: &InventorySource,
        package: InstalledPlugin,
    ) -> Result<InventoryPlugin, EngineError> {
        let mut secrets = Vec::new();
        for slot in secret_slots(&package.manifest).unwrap_or_default() {
            if let Some(value) = self.inventory_secret(source.id, &slot).await? {
                secrets.push((slot, value));
            }
        }
        let options = InventoryOptions {
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
            address: Some(source.address.clone()),
            secrets,
            refused_ports: refused_ports(),
        };
        let name = source.name.clone();
        let built = tokio::task::spawn_blocking(move || {
            InventoryPlugin::new(&package.manifest, &package.component, options)
        })
        .await
        .log_panic("engine.inventory_build");
        match built {
            Ok(Ok(plugin)) => Ok(plugin),
            Ok(Err(failure)) => Err(InventorySourceError::Failed {
                name,
                code: failure_code(failure.code()),
            }
            .into()),
            Err(error) => {
                tracing::warn!(%error, "an inventory could not be built");
                Err(InventorySourceError::Failed {
                    name,
                    code: failure_code(None),
                }
                .into())
            }
        }
    }

    /// The value of `slot` of source `id`, decrypted; `None` when none is stored.
    async fn inventory_secret(
        &self,
        id: InventorySourceId,
        slot: &str,
    ) -> Result<Option<SecretString>, EngineError> {
        let Some((row, cipher)) = self.db()?.inventory_secret(id, slot).await? else {
            return Ok(None);
        };
        let row = row.to_string();
        let field = FieldRef::new(INVENTORY_SECRETS_TABLE, INVENTORY_SECRET_COLUMN, &row);
        let by = crate::secrets::secret_use();
        match self
            .vault_handle()
            .await?
            .decrypt_string_for(&field, &cipher, by)
        {
            Ok(value) => {
                self.remember_secret(&value);
                Ok(Some(value))
            }
            Err(VaultError::Locked) => Err(EngineError::VaultLocked),
            Err(error) => Err(error.into()),
        }
    }
}

/// A host as the sync takes it: `ssh` opens the terminal, `sftp` the files; the folder path
/// split into its names; the note with the tags under it.
pub(super) fn entry(host: &InventoryHost) -> InventoryEntry {
    let (kind, default_view) = match host.protocol {
        Protocol::Ssh => (ConnectionKind::Ssh, DefaultView::Terminal),
        Protocol::Sftp => (ConnectionKind::Ssh, DefaultView::Files),
        Protocol::Ftp => (ConnectionKind::Ftp, DefaultView::Files),
        Protocol::Ftps => (ConnectionKind::Ftps, DefaultView::Files),
    };
    let tags = (!host.tags.is_empty()).then(|| format!("Tags: {}", host.tags.join(", ")));
    let notes = match (host.note.clone(), tags) {
        (Some(note), Some(tags)) => Some(format!("{note}\n{tags}")),
        (note, tags) => note.or(tags),
    };
    InventoryEntry {
        external_id: host.external_id.clone(),
        name: host.name.clone(),
        address: host.address.clone(),
        port: host.port,
        kind,
        default_view,
        user: host.user.clone(),
        folder: host
            .folder
            .split('/')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect(),
        notes,
    }
}

/// The numbers of a sync, and its first refusals by the host's name.
pub(super) fn report(
    source: InventorySourceId,
    preview: bool,
    listed: &InventoryList,
    entries: &[InventoryEntry],
    done: &InventorySync,
) -> InventoryReport {
    let (created, updated, unchanged, refused) = done.counts();
    let refusals = entries
        .iter()
        .zip(&done.statuses)
        .filter_map(|(entry, status)| match status {
            SyncStatus::Refused { code } => Some((entry.name.clone(), code.clone())),
            _ => None,
        })
        .take(MAX_REFUSALS_SHOWN)
        .collect();
    InventoryReport {
        source: Some(source),
        preview,
        created,
        updated,
        unchanged,
        missing: done.missing.len(),
        still_missing: done.still_missing,
        returned: done.returned.len(),
        skipped: listed.skipped,
        refused,
        refusals,
        folder: done.folder,
    }
}
