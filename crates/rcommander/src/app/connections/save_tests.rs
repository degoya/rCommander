//! Saving the form through `apply` against a real engine (RC-080-25): one save is one
//! `connections.save` - one change of the tree - and a refusal stores nothing, its message at
//! the field it is about.

use std::{sync::Arc, time::Duration};

use rc_db::{Db, SECRET_COLUMNS};
use rc_engine::{
    AuthMethod, Command, EngineEvent, Outcome, SecretKind, SecretParam, TreeItem, VaultStateKind,
};
use rc_vault::{KdfParams, ManualClock, SecretString, Vault, VaultOptions};
use tempfile::TempDir;
use tokio::sync::broadcast::Receiver;

use super::super::{
    ConnMsg, ConnectionManager,
    form::{Edit, Field},
};
use crate::app::{Message, harness::Harness};

/// The window over an engine whose vault is open and derives in milliseconds.
async fn harness() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let harness = Harness::on_db(root.path(), db.clone()).await;
    let engine = harness.app.engine();
    let vault = Vault::open_with(
        db,
        VaultOptions {
            clock: Arc::new(ManualClock::new()),
            kdf: KdfParams {
                m_cost_kib: 64,
                t_cost: 1,
                p_cost: 1,
            },
            failure_delay: Duration::ZERO,
            secret_columns: SECRET_COLUMNS,
        },
    )
    .await
    .expect("vault");
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

/// A new connection's form with its name and host typed.
async fn new_form(harness: &mut Harness, name: &str) {
    harness.press("Ctrl+F").await;
    harness.press("Insert").await;
    send(harness, ConnMsg::Edit(Edit::Name(name.into()))).await;
    send(harness, ConnMsg::Edit(Edit::Host("db.example.org".into()))).await;
}

async fn stored_count(harness: &Harness) -> usize {
    match harness
        .app
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    {
        Outcome::ConnectionTree { tree } => tree.connections.len(),
        other => panic!("the tree, got {other:?}"),
    }
}

/// The changes of connections the engine announced since `events` was last read.
fn tree_changes(events: &mut Receiver<EngineEvent>) -> usize {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter(|event| {
            matches!(
                event,
                EngineEvent::ConnectionTree {
                    item: TreeItem::Connection(_),
                    ..
                }
            )
        })
        .count()
}

#[tokio::test]
async fn a_save_with_a_secret_and_a_mark_is_one_command() {
    let (_root, mut harness) = harness().await;
    new_form(&mut harness, "db").await;
    assert_eq!(manager(&harness).vault, Some(VaultStateKind::Unlocked));
    let typed = SecretString::from("canary-gui-save-1");
    send(&mut harness, ConnMsg::Secret(SecretKind::Password, typed)).await;
    // A root connection is a server of the drive bar by default; the switch takes it away.
    send(&mut harness, ConnMsg::Toggle(Field::Favourite)).await;
    let mut events = harness.app.engine().subscribe();
    send(&mut harness, ConnMsg::Save).await;
    harness.settle().await;

    assert_eq!(
        tree_changes(&mut events),
        1,
        "the fields, the password and the mark in one command"
    );
    let manager = manager(&harness);
    assert!(manager.note.as_ref().is_some_and(|(_, error)| !error));
    let Some(TreeItem::Connection(id)) = manager.tree.cursor else {
        panic!("the cursor on the saved connection");
    };
    assert!(!manager.favourites.contains(&id));
    let Ok(Outcome::Connection { connection }) = harness
        .app
        .engine()
        .dispatch(Command::GetConnection { id })
        .await
    else {
        panic!("stored");
    };
    assert!(connection.secrets.has_password);
}

#[tokio::test]
async fn a_refused_key_stores_nothing_and_stands_at_the_key_file() {
    let (root, mut harness) = harness().await;
    new_form(&mut harness, "keyed").await;
    send(&mut harness, ConnMsg::Auth(AuthMethod::Key)).await;
    let missing = root.path().join("no_such_key");
    let path = missing.to_string_lossy().into_owned();
    send(&mut harness, ConnMsg::Edit(Edit::KeyPath(path))).await;
    send(&mut harness, ConnMsg::Toggle(Field::ImportKey)).await;
    send(&mut harness, ConnMsg::Save).await;
    harness.settle().await;

    let form = manager(&harness).form.as_ref().expect("the form");
    assert!(form.error(Field::KeyPath).is_some(), "{:?}", form.errors);
    assert_eq!(form.id(), None, "still a new connection");
    assert_eq!(
        stored_count(&harness).await,
        0,
        "no half-saved connection without its key"
    );

    // Without taking the key in, the same form saves once - not a second connection.
    send(&mut harness, ConnMsg::Toggle(Field::ImportKey)).await;
    send(&mut harness, ConnMsg::Save).await;
    harness.settle().await;
    assert_eq!(stored_count(&harness).await, 1);
    let form = manager(&harness).form.as_ref().expect("the form");
    assert!(form.errors.is_empty(), "{:?}", form.errors);
}
