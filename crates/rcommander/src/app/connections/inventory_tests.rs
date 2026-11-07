//! The inventories page (RC-100-39): the form fills from a source, a token typed is sent and
//! forgotten, the first sync of a source is its preview, Escape leaves.

use rc_engine::{
    Command, Outcome,
    inventory::{
        InventoryOutcome, InventoryPluginInfo, InventoryReport, InventorySource, InventorySourceId,
    },
};
use rc_vault::SecretString;

use super::{InventoryMsg, InventoryPage};
use crate::app::{
    Effect,
    connections::{ConnectionManager, jobs::ConnJob},
};

const PLUGIN: &str = "019d0000-0000-7000-8000-0000000000c1";

fn source(last_sync_at: Option<&str>) -> InventorySource {
    InventorySource {
        id: InventorySourceId(1),
        name: "NetBox".into(),
        plugin_id: PLUGIN.into(),
        address: "https://netbox.example.net/".into(),
        folder_id: None,
        last_sync_at: last_sync_at.map(str::to_owned),
        last_outcome: None,
        slots: vec!["api_token".into()],
    }
}

fn sources(last_sync_at: Option<&str>) -> InventoryMsg {
    InventoryMsg::Answer(Box::new(Ok(Outcome::Inventory(Box::new(
        InventoryOutcome::Sources {
            sources: vec![source(last_sync_at)],
            plugins: vec![InventoryPluginInfo {
                id: PLUGIN.into(),
                name: "NetBox".into(),
                version: "1.0.0".into(),
                slots: vec!["api_token".into()],
            }],
        },
    )))))
}

/// The command an effect runs for the page.
fn command(effect: &Effect) -> Option<&Command> {
    match effect {
        Effect::Connections(ConnJob::Inventory(command)) => Some(command.as_ref()),
        _ => None,
    }
}

fn open(last_sync_at: Option<&str>) -> ConnectionManager {
    let mut manager = ConnectionManager::default();
    let effects = manager.on_inventory(InventoryMsg::Open);
    assert!(matches!(
        effects.first().and_then(command),
        Some(Command::ListInventorySources {})
    ));
    manager.on_inventory(sources(last_sync_at));
    manager.on_inventory(InventoryMsg::Pick(InventorySourceId(1)));
    manager
}

fn page(manager: &ConnectionManager) -> &InventoryPage {
    manager.inventory.as_ref().expect("the page")
}

#[test]
fn a_source_fills_the_form_and_a_token_is_sent_once() {
    let mut manager = open(None);
    assert_eq!(page(&manager).name, "NetBox");
    assert_eq!(page(&manager).slots(), ["api_token"]);
    manager.on_inventory(InventoryMsg::Typed(
        "api_token".into(),
        SecretString::from("canary-token-5521"),
    ));
    let effects = manager.on_inventory(InventoryMsg::Save);
    let Some(Command::SaveInventorySource(request)) = effects.first().and_then(command) else {
        panic!("a save");
    };
    assert_eq!(request.id, Some(InventorySourceId(1)));
    assert_eq!(request.secrets.len(), 1);
    assert!(!format!("{request:?}").contains("canary-token-5521"));
    manager.on_inventory(InventoryMsg::Answer(Box::new(Ok(Outcome::Inventory(
        Box::new(InventoryOutcome::Saved {
            source: source(None),
        }),
    )))));
    assert_eq!(page(&manager).typed("api_token"), "", "forgotten once sent");
}

/// A source never synced is previewed first: Sync asks for the plan until one was shown.
#[test]
fn the_first_sync_is_its_preview() {
    let mut manager = open(None);
    let effects = manager.on_inventory(InventoryMsg::Sync);
    assert!(matches!(
        effects.first().and_then(command),
        Some(Command::RefreshInventory { preview: true, .. })
    ));
    manager.on_inventory(InventoryMsg::Answer(Box::new(Ok(Outcome::Inventory(
        Box::new(InventoryOutcome::Synced(InventoryReport {
            preview: true,
            created: 3,
            ..InventoryReport::default()
        })),
    )))));
    let effects = manager.on_inventory(InventoryMsg::Sync);
    assert!(matches!(
        effects.first().and_then(command),
        Some(Command::RefreshInventory { preview: false, .. })
    ));
    let synced = open(Some("2026-11-07T10:00:00.000Z"))
        .on_inventory(InventoryMsg::Sync)
        .first()
        .and_then(command)
        .cloned();
    assert!(matches!(
        synced,
        Some(Command::RefreshInventory { preview: false, .. })
    ));
}

#[test]
fn a_refusal_is_shown_and_escape_leaves() {
    let mut manager = open(None);
    manager.on_inventory(InventoryMsg::Answer(Box::new(Err(
        "plugin_inventory_too_large",
    ))));
    assert!(page(&manager).error.is_some());
    let escape: rc_engine::Chord = "Escape".parse().expect("a chord");
    manager.inventory_key(&escape, false);
    assert!(manager.inventory.is_none());
}
