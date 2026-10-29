use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use rc_db::Db;
use rc_vault::{ManualClock, MasterPassword, VaultStateKind};
use serde_json::json;
use tokio::time::timeout;

use super::*;
use crate::test_support;

/// An engine with a database and an unlocked vault.
async fn unlocked() -> (tempfile::TempDir, Engine) {
    let (root, engine) = test_support::engine(&[]).await;
    let db = Db::open_in_memory().await.expect("db");
    engine.attach_db(db.clone());
    let vault = Vault::open_with(
        db,
        test_support::fast_vault_options(&ManualClock::new(), &[]),
    )
    .await
    .expect("vault");
    vault
        .create(&MasterPassword::new("correct horse".to_owned()))
        .await
        .expect("create");
    engine.attach_vault(vault);
    (root, engine)
}

fn state(engine: &Engine) -> VaultStateKind {
    engine.vault().expect("a vault").status().state
}

#[test]
fn the_lock_and_suspend_notices_are_recognised_and_nothing_else() {
    assert_eq!(
        classify(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK),
        Some(SessionEvent::Locked)
    );
    assert_eq!(
        classify(WM_POWERBROADCAST, PBT_APMSUSPEND),
        Some(SessionEvent::Suspending)
    );
    // WTS_SESSION_UNLOCK, PBT_APMRESUMEAUTOMATIC, WM_PAINT.
    assert_eq!(classify(WM_WTSSESSION_CHANGE, 0x8), None);
    assert_eq!(classify(WM_POWERBROADCAST, 0x12), None);
    assert_eq!(classify(0x000F, WTS_SESSION_LOCK), None);
}

/// RC-080-11 (V2): the session's "locked" notice locks the vault and says so; with the setting
/// off it stays unlocked.
#[tokio::test]
async fn the_session_locking_locks_the_vault_unless_switched_off() {
    let (_root, engine) = unlocked().await;
    let mut events = engine.subscribe();
    assert!(engine.on_session_event(SessionEvent::Locked).await);
    assert_eq!(state(&engine), VaultStateKind::Locked);
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::VaultLocked
    );
    assert!(
        !engine.on_session_event(SessionEvent::Suspending).await,
        "locked already"
    );

    let (_root, engine) = unlocked().await;
    engine
        .set_protected_setting(LOCK_ON_SESSION_LOCK_KEY, &json!(false))
        .await
        .expect("switched off");
    assert!(!engine.on_session_event(SessionEvent::Suspending).await);
    assert_eq!(state(&engine), VaultStateKind::Unlocked);
}

/// The listener an engine starts hands each notice of the channel to it, and ends with it.
#[tokio::test]
async fn a_notice_on_the_channel_reaches_the_engine() {
    let (_root, engine) = unlocked().await;
    let (sender, receiver) = broadcast::channel(SESSION_BUFFER);
    let listener = tokio::spawn(listen(Arc::downgrade(&engine.inner), receiver));
    let mut events = engine.subscribe();
    sender.send(SessionEvent::Suspending).expect("a listener");
    let event = timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("handled")
        .expect("an event");
    assert_eq!(event, EngineEvent::VaultLocked);
    assert_eq!(state(&engine), VaultStateKind::Locked);

    drop(sender);
    timeout(Duration::from_secs(5), listener)
        .await
        .expect("the listener ends with its channel")
        .expect("no panic");
    let gone: Weak<Inner> = Weak::new();
    let (sender, receiver) = broadcast::channel(SESSION_BUFFER);
    let listener = tokio::spawn(listen(gone, receiver));
    sender.send(SessionEvent::Locked).expect("a listener");
    timeout(Duration::from_secs(5), listener)
        .await
        .expect("the listener ends without its engine")
        .expect("no panic");
}

/// MCP cannot switch the lock off: every `vault.*` key is protected.
#[test]
fn the_setting_is_protected() {
    assert!(crate::settings::is_protected(LOCK_ON_SESSION_LOCK_KEY));
    let spec = crate::settings::setting(LOCK_ON_SESSION_LOCK_KEY).expect("a schema row");
    assert!(spec.protected);
    assert_eq!(spec.default_value(), json!(true));
}
