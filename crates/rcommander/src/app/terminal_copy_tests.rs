//! The terminals copy and paste as the settings `terminal.copy_*`, `terminal.paste_ctrl_v`,
//! `terminal.osc52_copy` and `terminal.shift_selects` say (RC-040-07).

use rc_db::Db;
use rc_engine::{
    Command, EngineEvent,
    settings::{COPY_CTRL_C_KEY, COPY_ON_SELECT_KEY, OSC52_COPY_KEY, PASTE_CTRL_V_KEY},
};
use rc_term_widget::CopySettings;
use serde_json::json;

use super::load;
use crate::app::{
    Effect, Message,
    harness::{Harness, headless_engine},
    terminal_pane::TermMessage,
};

#[tokio::test]
async fn the_settings_read_as_windows_terminal_until_changed() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    assert_eq!(load(db.clone()).await, CopySettings::default());
    for (key, value) in [
        (COPY_CTRL_C_KEY, false),
        (COPY_ON_SELECT_KEY, true),
        (PASTE_CTRL_V_KEY, false),
        (OSC52_COPY_KEY, false),
    ] {
        engine
            .dispatch(Command::SetSetting {
                key: key.into(),
                value: json!(value),
            })
            .await
            .expect("stored");
    }
    let expected = CopySettings {
        ctrl_c: false,
        right_click: true,
        on_select: true,
        ctrl_v: false,
        osc52: false,
        shift_selects: true,
    };
    assert_eq!(load(db).await, expected);
}

#[tokio::test]
async fn a_change_is_read_and_reaches_the_terminals() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let changed = Message::Engine(EngineEvent::SettingChanged {
        key: COPY_ON_SELECT_KEY.into(),
    });
    assert!(
        matches!(harness.app.apply(changed)[..], [Effect::Task(_)]),
        "a change is read"
    );
    let copy = CopySettings {
        ctrl_c: false,
        right_click: false,
        on_select: true,
        ctrl_v: true,
        osc52: true,
        shift_selects: false,
    };
    harness
        .send(Message::Terminal(TermMessage::Copy(copy)))
        .await;
    assert_eq!(harness.app.terms.copy, copy);
}
