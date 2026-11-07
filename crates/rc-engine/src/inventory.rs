//! Server inventories (RC-100-39): an installed inventory plugin lists the hosts of a system
//! such as NetBox or Proxmox, and the engine keeps one folder of connections in step with it.
//!
//! A source (`rc_db::inventory`, migration 0026) names the plugin, the address the person
//! entered and confirmed, and the folder; its API token is a vault value of one of the plugin's
//! `secrets` slots, which the plugin never reads - the host puts it into a request to that
//! address in place of a `{{secret:<slot>}}` marker (`rc_plugin_host::InventoryPlugin`). A sync
//! builds the plugin for the call, asks `list`, and hands the checked hosts to
//! `Db::sync_inventory`; nothing is cached.
//!
//! `inventory.sources` and `inventory.refresh` reach MCP; setting a source up, changing it or
//! storing its token is the person's (`GuiOnly`). The first sync of a source is previewed in the
//! window; the schedule (`inventory.refresh_minutes`, `schedule.rs`) syncs only sources synced
//! before.

mod commands;
mod error;
mod schedule;

use serde::{Deserialize, Serialize};

pub use commands::MAX_REFUSALS_SHOWN;
pub use error::InventorySourceError;
pub use rc_db::inventory::{
    InventoryEntry, InventoryLink, InventorySource, InventorySourceDraft, InventorySourceId,
    InventorySync, SyncStatus,
};
pub use schedule::SCHEDULE_TICK;

use crate::{FolderId, SecretParam};

/// The parameters of `inventory.source_save`: a new source when `id` is `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySourceRequest {
    #[serde(default)]
    pub id: Option<InventorySourceId>,
    pub name: String,
    /// The inventory plugin's id, as `inventory.sources` lists the plugins.
    pub plugin_id: String,
    /// What the plugin reaches: `https://netbox.example.net/`.
    pub address: String,
    /// The folder its connections go into; `None` makes one named after the source.
    #[serde(default)]
    pub folder_id: Option<FolderId>,
    /// Values for the plugin's slots, such as its API token; a slot left out keeps its value.
    #[serde(default)]
    pub secrets: Vec<InventorySecret>,
}

/// The value of one slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySecret {
    pub slot: String,
    pub value: SecretParam,
}

/// One installed inventory plugin, for the form: what it may reach and which slots it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryPluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    /// The slots of its `secrets` grant; a value for each is asked in the form.
    pub slots: Vec<String>,
}

/// What a sync did or would do, in numbers and the hosts it refused - never a secret.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryReport {
    pub source: Option<InventorySourceId>,
    /// Only the plan: nothing was written.
    pub preview: bool,
    /// Created, or new in a preview.
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
    /// Marked (or to be marked) as no longer in the inventory.
    pub missing: usize,
    /// Marked before and still missing.
    pub still_missing: usize,
    /// Marked before and listed again.
    pub returned: usize,
    /// Hosts the plugin host left out (a control character, `..` in the folder, port 0, ...),
    /// and those the sync refused (a name taken, a duplicate id).
    pub skipped: usize,
    pub refused: usize,
    /// The first refusals: the host's name and the code why.
    pub refusals: Vec<(String, String)>,
    /// The source's folder.
    pub folder: Option<FolderId>,
}

/// The answer of an `inventory.*` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum InventoryOutcome {
    /// `inventory.sources`: the sources by name, and the installed inventory plugins.
    Sources {
        sources: Vec<InventorySource>,
        plugins: Vec<InventoryPluginInfo>,
    },
    /// `inventory.refresh`: what the sync did, or would do in a preview.
    Synced(InventoryReport),
    /// `inventory.source_save`: the source as stored.
    Saved { source: InventorySource },
    /// `inventory.source_delete`.
    Deleted { source: InventorySourceId },
}

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
