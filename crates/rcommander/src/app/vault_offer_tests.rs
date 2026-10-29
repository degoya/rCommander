//! The offer to remove the vault through `apply`, against a real engine on an in-memory
//! database: clearing the last secret offers it, Remove removes the vault, Keep and Escape leave
//! it, the keys belong to the open offer.

use rc_engine::{Command, ConnectionDraft, Outcome, PanelId, VaultStateKind};
use tempfile::TempDir;

use super::VaultOfferMsg;
use crate::app::{
    Message,
    harness::{self, Harness},
};

/// An app over an engine with a vault and one stored password, nothing offered yet.
async fn harness() -> (TempDir, Harness, rc_engine::ConnectionId) {
    let (root, mut harness) = Harness::temp_with_db().await;
    let engine = harness.app.engine().clone();
    engine
        .dispatch(Command::CreateVault {
            password: "master-password-1".into(),
        })
        .await
        .expect("vault");
    let id = match engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("db", "db.example.org"),
        })
        .await
        .expect("created")
    {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    };
    engine
        .dispatch(harness::password_change(id, Some("canary-password-1")))
        .await
        .expect("stored");
    // The window was not open while the vault and the secret were made: it saw none of it.
    harness.miss_events();
    assert!(!harness.app.vault_offer);
    (root, harness, id)
}

/// The last secret cleared from elsewhere (an MCP call, the manager), then the window catching up.
async fn clear_last_secret(harness: &mut Harness, id: rc_engine::ConnectionId) {
    harness
        .app
        .engine()
        .dispatch(harness::password_change(id, None))
        .await
        .expect("cleared");
    harness.settle().await;
}

async fn vault_state(harness: &Harness) -> VaultStateKind {
    match harness
        .app
        .engine()
        .dispatch(Command::GetVaultStatus)
        .await
        .expect("status")
    {
        Outcome::Vault(status) => status.state,
        other => panic!("a vault status, got {other:?}"),
    }
}

#[tokio::test]
async fn clearing_the_last_secret_offers_to_remove_the_vault() {
    let (_root, mut harness, id) = harness().await;
    clear_last_secret(&mut harness, id).await;
    assert!(harness.app.vault_offer, "the offer is open");

    // A key typed while the offer is open reaches nothing below it.
    harness.press("Tab").await;
    harness.press("Enter").await;
    assert_eq!(harness.app.active, PanelId::Left, "the panels do not move");
    assert!(harness.app.vault_offer);
    assert_eq!(vault_state(&harness).await, VaultStateKind::Unlocked);

    harness
        .send(Message::VaultOffer(VaultOfferMsg::Remove))
        .await;
    assert!(!harness.app.vault_offer);
    assert_eq!(vault_state(&harness).await, VaultStateKind::Absent);
    assert_eq!(harness.app.pane(PanelId::Left).error, None);
}

#[tokio::test]
async fn keep_and_escape_dismiss_the_offer_and_the_vault_stays() {
    let (_root, mut harness, id) = harness().await;
    clear_last_secret(&mut harness, id).await;
    harness.send(Message::VaultOffer(VaultOfferMsg::Keep)).await;
    assert!(!harness.app.vault_offer);
    assert_eq!(vault_state(&harness).await, VaultStateKind::Unlocked);

    harness
        .send(Message::Engine(rc_engine::EngineEvent::VaultUnused))
        .await;
    assert!(harness.app.vault_offer);
    harness.press("Escape").await;
    assert!(!harness.app.vault_offer);
    assert_eq!(vault_state(&harness).await, VaultStateKind::Unlocked);

    // A button of a closed offer does nothing.
    harness
        .send(Message::VaultOffer(VaultOfferMsg::Remove))
        .await;
    assert_eq!(vault_state(&harness).await, VaultStateKind::Unlocked);
}

#[tokio::test]
async fn a_vault_removed_elsewhere_closes_the_offer() {
    let (_root, mut harness, id) = harness().await;
    clear_last_secret(&mut harness, id).await;
    assert!(harness.app.vault_offer);
    harness
        .app
        .engine()
        .dispatch(Command::RemoveVault)
        .await
        .expect("removed");
    harness.settle().await;
    assert!(!harness.app.vault_offer);
}

#[tokio::test]
async fn a_secret_stored_meanwhile_keeps_the_vault() {
    let (_root, mut harness, id) = harness().await;
    clear_last_secret(&mut harness, id).await;
    harness
        .app
        .engine()
        .dispatch(harness::password_change(id, Some("canary-password-2")))
        .await
        .expect("stored again");
    harness.settle().await;

    harness
        .send(Message::VaultOffer(VaultOfferMsg::Remove))
        .await;
    assert!(!harness.app.vault_offer);
    assert_eq!(vault_state(&harness).await, VaultStateKind::Unlocked);
    assert!(
        harness.app.pane(PanelId::Left).error.is_some(),
        "the refusal shows in the status line"
    );
}
