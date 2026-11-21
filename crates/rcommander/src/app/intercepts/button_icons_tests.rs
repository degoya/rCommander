//! The icons on the dialogs' buttons follow the setting `appearance.button_icons` (RC-110-39).

use rc_db::Db;
use rc_engine::{Command, EngineEvent, settings::BUTTON_ICONS_KEY};
use serde_json::json;

use super::icons_shown;
use crate::app::{
    Effect, Message,
    harness::{Harness, headless_engine},
};

#[tokio::test]
async fn the_setting_is_read_on_unless_switched_off() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    assert!(icons_shown(db.clone()).await, "the default");
    engine
        .dispatch(Command::SetSetting {
            key: BUTTON_ICONS_KEY.into(),
            value: json!(false),
        })
        .await
        .expect("stored");
    assert!(!icons_shown(db).await);
}

#[tokio::test]
async fn a_change_of_the_setting_is_read() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let changed = Message::Engine(EngineEvent::SettingChanged {
        key: BUTTON_ICONS_KEY.into(),
    });
    assert!(
        matches!(harness.app.apply(changed)[..], [Effect::Task(_)]),
        "a change is read"
    );
    let other = Message::Engine(EngineEvent::SettingChanged {
        key: "appearance.dialog_windows".into(),
    });
    assert!(harness.app.button_icons_intercept(&other).is_none());
}
