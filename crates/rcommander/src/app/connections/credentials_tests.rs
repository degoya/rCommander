//! Shared credentials in the manager against a real engine (RC-050-12): a new one saved from its
//! own form, a connection pointed at it, and the setting that takes them out of the window.

use std::{sync::Arc, time::Duration};

use rc_db::{Db, SECRET_COLUMNS};
use rc_engine::{Command, CredentialId, Outcome, SecretKind, SecretParam, TreeItem};
use rc_vault::{KdfParams, ManualClock, SecretString, Vault, VaultOptions};
use tempfile::TempDir;

use super::super::{ConnMsg, ConnectionManager, form::Edit};
use super::*;
use crate::app::{Message, harness::Harness};

/// The window over an engine whose vault is open and derives in milliseconds.
async fn harness() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let harness = Harness::on_db(root.path(), db.clone()).await;
    let engine = harness.app.engine();
    let options = VaultOptions {
        clock: Arc::new(ManualClock::new()),
        kdf: KdfParams {
            m_cost_kib: 64,
            t_cost: 1,
            p_cost: 1,
        },
        failure_delay: Duration::ZERO,
        secret_columns: SECRET_COLUMNS,
    };
    let vault = Vault::open_with(db, options).await.expect("vault");
    assert!(engine.attach_vault(vault));
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from("correct horse battery"),
        })
        .await
        .expect("vault created");
    (root, harness)
}

fn manager(harness: &Harness) -> &ConnectionManager {
    harness
        .app
        .connections
        .as_ref()
        .expect("the manager is open")
}

async fn send(harness: &mut Harness, message: ConnMsg) {
    harness.send(Message::Connections(message)).await;
}

/// A shared credential `ops` for `deploy` with a password, saved from its form.
async fn saved_credential(harness: &mut Harness) -> CredentialId {
    harness.press("Ctrl+F").await;
    harness.settle().await;
    send(harness, ConnMsg::NewCredential).await;
    let form = manager(harness).form.as_ref().expect("a form");
    assert!(form.is_credential());
    assert_eq!(
        form.fields(),
        [
            Field::Name,
            Field::User,
            Field::Password,
            Field::KeyPath,
            Field::ImportKey,
            Field::Passphrase,
        ]
    );
    send(harness, ConnMsg::Edit(Edit::Name("ops".into()))).await;
    send(harness, ConnMsg::Edit(Edit::User("deploy".into()))).await;
    let typed = SecretString::from("canary-gui-shared-1");
    send(harness, ConnMsg::Secret(SecretKind::Password, typed)).await;
    send(harness, ConnMsg::Save).await;
    harness.settle().await;
    let manager = manager(harness);
    let Some(TreeItem::Credential(id)) = manager.tree.cursor else {
        panic!("the cursor on the saved credential: {:?}", manager.note);
    };
    id
}

#[tokio::test]
async fn a_credential_is_saved_from_its_form_and_shown_in_the_tree() {
    let (_root, mut harness) = harness().await;
    let id = saved_credential(&mut harness).await;
    let manager = manager(&harness);
    let entry = manager.tree.credential(id).expect("in the tree");
    assert_eq!(entry.username.as_deref(), Some("deploy"));
    assert!(entry.secrets.has_password);
    let row = manager
        .tree
        .rows()
        .into_iter()
        .find(|row| row.item == TreeItem::Credential(id))
        .expect("a row");
    assert_eq!((row.name, row.detail), ("ops", "deploy"));
    let form = manager.form.as_ref().expect("its form");
    assert_eq!(form.credential_id(), Some(id));
    assert!(!form.dirty);
}

#[tokio::test]
async fn a_connection_points_at_a_credential_instead_of_its_own_secrets() {
    let (_root, mut harness) = harness().await;
    let id = saved_credential(&mut harness).await;
    send(&mut harness, ConnMsg::NewConnection).await;
    send(&mut harness, ConnMsg::Edit(Edit::Name("web".into()))).await;
    send(
        &mut harness,
        ConnMsg::Edit(Edit::Host("web.example.org".into())),
    )
    .await;
    let form = manager(&harness).form.as_ref().expect("a form");
    assert!(form.fields().contains(&Field::Credential), "offered");
    assert!(form.fields().contains(&Field::Password));
    send(&mut harness, ConnMsg::Shared(Some(id))).await;
    let form = manager(&harness).form.as_ref().expect("a form");
    assert!(!form.fields().contains(&Field::Password), "no own secret");
    send(&mut harness, ConnMsg::Save).await;
    harness.settle().await;
    let Some(TreeItem::Connection(web)) = manager(&harness).tree.cursor else {
        panic!("the cursor on the connection");
    };
    let Ok(Outcome::Connection { connection }) = harness
        .app
        .engine()
        .dispatch(Command::GetConnection { id: web })
        .await
    else {
        panic!("stored");
    };
    assert_eq!(connection.credential_id, Some(id));
    let entry = manager(&harness).tree.credential(id).expect("in the tree");
    assert_eq!(entry.connections, 1, "the delete question counts it");
}

#[tokio::test]
async fn with_the_setting_off_the_window_offers_no_credentials() {
    let (_root, mut harness) = harness().await;
    let id = saved_credential(&mut harness).await;
    harness
        .app
        .engine()
        .set_protected_setting(rc_engine::SHARED_CREDENTIALS_KEY, &false.into())
        .await
        .expect("off");
    harness.settle().await;
    let manager = manager(&harness);
    assert!(!manager.tree.offer_credentials);
    assert!(
        !manager
            .tree
            .rows()
            .iter()
            .any(|row| row.item == TreeItem::Credential(id))
    );
    assert!(
        !manager.form.as_ref().is_some_and(Form::is_credential),
        "its form went with its row"
    );
    send(&mut harness, ConnMsg::NewCredential).await;
    let form = self::manager(&harness).form.as_ref();
    assert!(!form.is_some_and(Form::is_credential), "no new credential");
}
