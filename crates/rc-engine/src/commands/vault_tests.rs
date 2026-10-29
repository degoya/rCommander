use std::time::Duration;

use rc_db::Db;
use rc_vault::{ManualClock, MasterPassword, VaultStateKind};
use tokio::time::timeout;

use super::*;
use crate::Command;
use crate::test_support;
use crate::test_support::engine;

async fn vault(clock: &ManualClock) -> Vault {
    let db = Db::open_in_memory().await.expect("db");
    Vault::open_with(db, test_support::fast_vault_options(clock, &[]))
        .await
        .expect("vault")
}

async fn status(engine: &Engine) -> VaultStatus {
    match engine.dispatch(Command::GetVaultStatus).await {
        Ok(Outcome::Vault(status)) => status,
        other => panic!("a vault status, got {other:?}"),
    }
}

#[tokio::test]
async fn an_engine_without_a_vault_reports_absent() {
    let (_root, engine) = engine(&[]).await;
    assert!(engine.vault().is_none());
    assert_eq!(status(&engine).await, VaultStatus::absent());
    let mut events = engine.subscribe();
    engine.dispatch(Command::LockVault).await.expect("lock");
    assert!(events.try_recv().is_err(), "nothing was locked");
}

#[tokio::test]
async fn status_and_lock_go_through_the_engine() {
    let (_root, engine) = engine(&[]).await;
    let clock = ManualClock::new();
    assert!(engine.attach_vault(vault(&clock).await));
    assert!(!engine.attach_vault(vault(&clock).await), "one vault only");
    assert_eq!(status(&engine).await.state, VaultStateKind::Absent);

    let attached = engine.vault().expect("attached");
    attached
        .create(&MasterPassword::new("pw".to_owned()))
        .await
        .expect("create");
    let unlocked = status(&engine).await;
    assert_eq!(unlocked.state, VaultStateKind::Unlocked);
    assert_eq!(unlocked.locks_in_secs, Some(15 * 60));

    let mut events = engine.subscribe();
    let Ok(Outcome::Vault(locked)) = engine.dispatch(Command::LockVault).await else {
        panic!("a vault status");
    };
    assert_eq!(locked.state, VaultStateKind::Locked);
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::VaultLocked
    );
    assert_eq!(status(&engine).await.state, VaultStateKind::Locked);
}

#[tokio::test]
async fn the_timer_locks_an_idle_vault_and_says_so() {
    let (_root, engine) = engine(&[]).await;
    let clock = ManualClock::new();
    let idle = vault(&clock).await;
    idle.set_idle_lock_minutes(1).await.expect("setting");
    idle.create(&MasterPassword::new("pw".to_owned()))
        .await
        .expect("create");
    clock.advance(Duration::from_secs(61));

    let mut events = engine.subscribe();
    assert!(engine.attach_vault(idle));
    let event = timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("the timer ran")
        .expect("an event");
    assert_eq!(event, EngineEvent::VaultLocked);
    assert_eq!(status(&engine).await.state, VaultStateKind::Locked);
}

#[test]
fn the_status_outcome_carries_no_key_material() {
    let outcome = Outcome::Vault(VaultStatus::absent());
    assert_eq!(
        serde_json::to_value(&outcome).expect("serializes"),
        serde_json::json!({ "kind": "vault", "state": "absent", "idle_lock_minutes": 15 })
    );
}

/// An engine on the database file `path` with its own vault, as a second rCommander on the same
/// profile has.
async fn engine_on(path: &std::path::Path) -> (tempfile::TempDir, Engine) {
    let (root, engine) = engine(&[]).await;
    let db = Db::open(path).await.expect("db");
    engine.attach_db(db.clone());
    let vault = test_support::fast_vault(db).await;
    engine.attach_vault(vault);
    (root, engine)
}

fn secret(value: &str) -> SecretParam {
    SecretParam::from(value)
}

/// RC-080-11 (V4): two engines on one database. After the first changed the master password,
/// the second - still holding the old key - is refused the write (`vault_changed`) and locks; no
/// secret is lost, and unlocked with the new password it stores again.
#[tokio::test]
async fn a_stale_key_is_refused_after_another_engine_changed_the_password() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(rc_db::DB_FILE);
    let (_one_root, one) = engine_on(&path).await;
    let (_two_root, two) = engine_on(&path).await;
    one.dispatch(Command::CreateVault {
        password: secret("first password"),
    })
    .await
    .expect("create");
    two.dispatch(Command::UnlockVault {
        password: secret("first password"),
    })
    .await
    .expect("the second unlocks too");
    let draft = crate::ConnectionDraft::new("web", "example.org");
    let Ok(Outcome::Connection { connection }) = one
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    else {
        panic!("a connection");
    };
    let web = connection.id;
    let password = crate::SecretRef {
        connection: web,
        kind: crate::SecretKind::Password,
    };
    two.dispatch(test_support::store_secret(
        web,
        crate::SecretKind::Password,
        secret("stored-by-two"),
    ))
    .await
    .expect("stored under the first password");

    one.dispatch(Command::ChangeMasterPassword {
        current: secret("first password"),
        new: secret("second password"),
    })
    .await
    .expect("changed by the first");
    let mut events = two.subscribe();
    let refused = two
        .dispatch(test_support::store_secret(
            web,
            crate::SecretKind::Password,
            secret("written-with-the-old-key"),
        ))
        .await
        .expect_err("a stale key");
    assert_eq!(refused.code(), "vault_changed");
    assert_eq!(status(&two).await.state, VaultStateKind::Locked);
    assert!(
        std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| event == EngineEvent::VaultLocked),
        "the window learns that it locked"
    );

    let kept = one
        .resolve_secret(password)
        .await
        .expect("still readable")
        .expect("stored");
    assert_eq!(
        rc_vault::ExposeSecret::expose_secret(&kept),
        "stored-by-two"
    );
    two.dispatch(Command::UnlockVault {
        password: secret("second password"),
    })
    .await
    .expect("unlocked with the new password");
    two.dispatch(test_support::store_secret(
        web,
        crate::SecretKind::Password,
        secret("written-with-the-new-key"),
    ))
    .await
    .expect("stored again");
    let read = one
        .resolve_secret(password)
        .await
        .expect("readable by the first")
        .expect("stored");
    assert_eq!(
        rc_vault::ExposeSecret::expose_secret(&read),
        "written-with-the-new-key"
    );
}
