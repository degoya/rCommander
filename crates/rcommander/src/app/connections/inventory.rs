//! The page of the server inventories (RC-100-39), opened from under the tree: the sources on
//! the left, the form of one on the right - its name, the inventory plugin, the address it
//! reaches and the values of the plugin's slots (its API token), typed hidden and never shown
//! once stored. A source is previewed before its first sync, as an import is (RC-050-11): Preview
//! shows what a sync would create, update and mark, Sync carries it out. A host that left the
//! inventory is marked in the database and counted on the page, never deleted.
//!
//! The schedule (`inventory.refresh_minutes`) runs beside the window, not on this page:
//! [`subscription`] asks the engine every minute to sync the sources that are due.

use std::hash::{Hash, Hasher};

use iced::{
    Subscription,
    futures::{Stream, stream},
};
use rc_engine::{
    Chord, Command, Engine, Outcome, SecretParam,
    inventory::{
        InventoryOutcome, InventoryPluginInfo, InventoryReport, InventorySecret, InventorySource,
        InventorySourceId, InventorySourceRequest,
    },
};
use rc_vault::{ExposeSecret as _, SecretString};

use super::{
    ConnMsg, ConnectionManager,
    jobs::ConnJob,
    preview::{PageKey, page_key},
};
use crate::{
    app::{Effect, Message},
    i18n::error_text,
};

/// The open page.
#[derive(Debug, Default)]
pub struct InventoryPage {
    pub sources: Vec<InventorySource>,
    pub plugins: Vec<InventoryPluginInfo>,
    /// The source in the form; `None` for a new one.
    pub editing: Option<InventorySourceId>,
    pub name: String,
    /// The inventory plugin's id.
    pub plugin: Option<String>,
    pub address: String,
    /// What is typed into each slot's field now; a stored value is never read back.
    pub typed: Vec<(String, SecretString)>,
    /// What the last preview or sync did.
    pub report: Option<InventoryReport>,
    /// A command runs.
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InventoryMsg {
    /// The button under the tree.
    Open,
    /// Back to the tree.
    Back,
    /// A source into the form.
    Pick(InventorySourceId),
    /// An empty form for a new source.
    New,
    Name(String),
    Plugin(String),
    Address(String),
    /// A slot's field: its name and what is typed.
    Typed(String, SecretString),
    Save,
    Preview,
    Sync,
    Delete,
    /// The engine answered.
    Answer(Box<Result<Outcome, &'static str>>),
    /// The schedule synced a source: the list is read again.
    Scheduled,
}

fn run(command: Command) -> Effect {
    Effect::Connections(ConnJob::Inventory(Box::new(command)))
}

fn list() -> Effect {
    run(Command::ListInventorySources {})
}

impl InventoryPage {
    /// The slots of the chosen plugin, for the form's fields.
    pub fn slots(&self) -> &[String] {
        self.plugin
            .as_deref()
            .and_then(|id| self.plugins.iter().find(|plugin| plugin.id == id))
            .map(|plugin| plugin.slots.as_slice())
            .unwrap_or_default()
    }

    /// What is typed into `slot`.
    pub fn typed(&self, slot: &str) -> &str {
        self.typed
            .iter()
            .find(|(name, _)| name == slot)
            .map_or("", |(_, value)| value.expose_secret())
    }

    /// The source in the form, as stored.
    pub fn current(&self) -> Option<&InventorySource> {
        let id = self.editing?;
        self.sources.iter().find(|source| source.id == id)
    }

    /// Whether the form can be saved: a name, a plugin and an address.
    pub fn can_save(&self) -> bool {
        !self.busy
            && !self.name.trim().is_empty()
            && self.plugin.is_some()
            && !self.address.trim().is_empty()
    }

    fn fill(&mut self, id: InventorySourceId) {
        let Some(source) = self.sources.iter().find(|source| source.id == id).cloned() else {
            return;
        };
        self.editing = Some(id);
        self.name = source.name;
        self.plugin = Some(source.plugin_id);
        self.address = source.address;
        self.typed.clear();
        self.report = None;
        self.error = None;
    }

    fn clear(&mut self) {
        self.editing = None;
        self.name.clear();
        self.plugin = self.plugins.first().map(|plugin| plugin.id.clone());
        self.address.clear();
        self.typed.clear();
        self.report = None;
        self.error = None;
    }

    fn request(&self) -> Option<InventorySourceRequest> {
        Some(InventorySourceRequest {
            id: self.editing,
            name: self.name.trim().to_owned(),
            plugin_id: self.plugin.clone()?,
            address: self.address.trim().to_owned(),
            folder_id: self.current().and_then(|source| source.folder_id),
            secrets: self
                .typed
                .iter()
                .filter(|(_, value)| !value.expose_secret().is_empty())
                .map(|(slot, value)| InventorySecret {
                    slot: slot.clone(),
                    value: SecretParam::new(value.expose_secret().to_owned()),
                })
                .collect(),
        })
    }
}

impl ConnectionManager {
    pub(super) fn on_inventory(&mut self, message: InventoryMsg) -> Vec<Effect> {
        if let InventoryMsg::Open = message {
            self.inventory = Some(InventoryPage::default());
            self.pending = None;
            return vec![list()];
        }
        let asked_preview = matches!(message, InventoryMsg::Preview);
        let Some(page) = self.inventory.as_mut() else {
            return Vec::new();
        };
        match message {
            InventoryMsg::Open => {}
            InventoryMsg::Back => self.inventory = None,
            InventoryMsg::Pick(id) => page.fill(id),
            InventoryMsg::New => page.clear(),
            InventoryMsg::Name(name) => page.name = name,
            InventoryMsg::Plugin(id) => {
                page.plugin = Some(id);
                page.typed.clear();
            }
            InventoryMsg::Address(address) => page.address = address,
            InventoryMsg::Typed(slot, value) => {
                page.typed.retain(|(name, _)| *name != slot);
                page.typed.push((slot, value));
            }
            InventoryMsg::Save => {
                if let (true, Some(request)) = (page.can_save(), page.request()) {
                    page.busy = true;
                    page.error = None;
                    return vec![run(Command::SaveInventorySource(request))];
                }
            }
            InventoryMsg::Preview | InventoryMsg::Sync => {
                let Some(source) = page.editing.filter(|_| !page.busy) else {
                    return Vec::new();
                };
                // The first sync of a source follows its preview.
                let preview = asked_preview
                    || (page
                        .current()
                        .is_some_and(|source| source.last_sync_at.is_none())
                        && page.report.as_ref().is_none_or(|report| !report.preview));
                page.busy = true;
                page.error = None;
                return vec![run(Command::RefreshInventory { source, preview })];
            }
            InventoryMsg::Delete => {
                if let Some(source) = page.editing.filter(|_| !page.busy) {
                    page.busy = true;
                    return vec![run(Command::DeleteInventorySource { source })];
                }
            }
            InventoryMsg::Answer(result) => return Self::inventory_answer(page, *result),
            InventoryMsg::Scheduled => return vec![list()],
        }
        Vec::new()
    }

    fn inventory_answer(
        page: &mut InventoryPage,
        result: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        page.busy = false;
        let outcome = match result {
            Ok(Outcome::Inventory(outcome)) => *outcome,
            Ok(_) => return Vec::new(),
            Err(code) => {
                page.error = Some(error_text(code));
                return vec![list()];
            }
        };
        match outcome {
            InventoryOutcome::Sources { sources, plugins } => {
                page.sources = sources;
                page.plugins = plugins;
                if page.plugin.is_none() {
                    page.plugin = page.plugins.first().map(|plugin| plugin.id.clone());
                }
                Vec::new()
            }
            InventoryOutcome::Saved { source } => {
                page.editing = Some(source.id);
                page.typed.clear();
                vec![list()]
            }
            InventoryOutcome::Synced(report) => {
                page.report = Some(report);
                vec![list()]
            }
            InventoryOutcome::Deleted { .. } => {
                page.clear();
                vec![list()]
            }
        }
    }

    /// A key while the page is shown: Escape goes back to the tree, Enter saves.
    pub(super) fn inventory_key(&mut self, chord: &Chord, captured: bool) -> Vec<Effect> {
        match page_key(chord, captured) {
            Some(PageKey::Leave) => {
                self.inventory = None;
                Vec::new()
            }
            Some(PageKey::Import) => self.on_inventory(InventoryMsg::Save),
            None => Vec::new(),
        }
    }
}

/// The message of the page's controls.
pub fn msg(message: InventoryMsg) -> Message {
    Message::Connections(ConnMsg::Inventory(message))
}

/// The schedule of `inventory.refresh_minutes`: every minute the engine syncs the sources that
/// are due (`Engine::run_due_inventories`); what changes reaches the window as the tree's events.
pub fn subscription(engine: &Engine) -> Subscription<Message> {
    Subscription::run_with(Schedule(engine.clone()), schedule)
}

/// The engine whose inventories the window keeps in step; one per window.
struct Schedule(Engine);

impl Hash for Schedule {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "rcommander.inventory-schedule".hash(state);
    }
}

fn schedule(schedule: &Schedule) -> impl Stream<Item = Message> + use<> {
    stream::unfold(schedule.0.clone(), |engine| async move {
        loop {
            tokio::time::sleep(rc_engine::inventory::SCHEDULE_TICK).await;
            if engine.run_due_inventories().await > 0 {
                break;
            }
        }
        Some((msg(InventoryMsg::Scheduled), engine))
    })
}

#[cfg(test)]
#[path = "inventory_tests.rs"]
mod tests;
