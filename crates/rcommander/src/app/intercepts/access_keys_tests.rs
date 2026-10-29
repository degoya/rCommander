//! The underline of the access keys follows Alt and the setting `keys.underline_access_keys`
//! (RC-080-16).

use iced::keyboard::Modifiers;
use rc_db::Db;
use rc_engine::{Command, EngineEvent, settings::UNDERLINE_ACCESS_KEYS_KEY};
use serde_json::json;

use super::underline_always;
use crate::app::harness::headless_engine;
use crate::{
    app::{Effect, Message, file_ops::FileOpsMsg, harness::Harness},
    theme::mnemonic,
};

#[tokio::test]
async fn the_setting_is_read_off_unless_switched_on() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    assert!(!underline_always(db.clone()).await, "the default");
    engine
        .dispatch(Command::SetSetting {
            key: UNDERLINE_ACCESS_KEYS_KEY.into(),
            value: json!(true),
        })
        .await
        .expect("stored");
    assert!(underline_always(db).await);
}

#[tokio::test]
async fn alt_and_the_setting_show_the_underline() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let changed = Message::Engine(EngineEvent::SettingChanged {
        key: UNDERLINE_ACCESS_KEYS_KEY.into(),
    });
    assert!(
        matches!(harness.app.apply(changed)[..], [Effect::Task(_)]),
        "a change is read"
    );
    assert!(!mnemonic::shown(), "off at the start");
    let modifiers = |held| Message::FileOps(FileOpsMsg::Modifiers(held));
    harness.send(modifiers(Modifiers::ALT)).await;
    assert!(mnemonic::shown(), "while Alt is held");
    harness.send(modifiers(Modifiers::empty())).await;
    assert!(!mnemonic::shown());
    harness.send(Message::AccessKeys(true)).await;
    assert!(mnemonic::shown(), "always");
    harness.send(Message::AccessKeys(false)).await;
    assert!(!mnemonic::shown());
}
