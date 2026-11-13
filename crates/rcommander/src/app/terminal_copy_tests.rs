//! The terminals copy and paste as the settings `terminal.copy_*`, `terminal.paste_ctrl_v`,
//! `terminal.osc52_copy` and `terminal.shift_selects` say (RC-040-07).

use rc_db::Db;
use rc_engine::{
    Command, EngineEvent,
    settings::{
        COPY_CTRL_C_KEY, COPY_ON_SELECT_KEY, OSC52_COPY_KEY, PASTE_CTRL_V_KEY,
        PASTE_RIGHT_CLICK_KEY,
    },
};
use rc_term_widget::CopySettings;
use serde_json::json;

use super::{copy_allowed, load};
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
        (PASTE_RIGHT_CLICK_KEY, false),
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
        right_click_pastes: false,
        on_select: true,
        ctrl_v: false,
        osc52: false,
        osc52_remote: true,
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
        right_click_pastes: false,
        on_select: true,
        ctrl_v: true,
        osc52: true,
        osc52_remote: false,
        shift_selects: false,
    };
    harness
        .send(Message::Terminal(TermMessage::Copy(copy)))
        .await;
    assert_eq!(harness.app.terms.copy, copy);
}

#[test]
fn a_program_copy_needs_the_setting_room_a_shown_terminal_and_for_ssh_its_own_setting() {
    let copy = CopySettings::default();
    let limit = rc_term::osc::MAX_CLIPBOARD;
    assert!(copy_allowed(copy, 5, true, false));
    assert!(
        copy_allowed(copy, limit, true, true),
        "on for SSH by default"
    );
    assert!(!copy_allowed(copy, limit + 1, true, false));
    assert!(!copy_allowed(copy, 5, false, false), "a hidden terminal's");
    let no_remote = CopySettings {
        osc52_remote: false,
        ..copy
    };
    assert!(!copy_allowed(no_remote, 5, true, true));
    assert!(
        copy_allowed(no_remote, 5, true, false),
        "a local terminal's still"
    );
    let off = CopySettings {
        osc52: false,
        ..copy
    };
    assert!(!copy_allowed(off, 5, true, false));
}

/// RC-080-65 through `apply`: the shown terminal's copy is written and named in the status
/// line; a hidden one's and an overlong one are dropped.
#[cfg(unix)]
#[tokio::test]
async fn only_the_shown_terminal_sets_the_clipboard_and_says_so() {
    use rc_engine::PanelId;
    use rc_term_widget::Event as WidgetEvent;

    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let open = |harness: &Harness| {
        let engine = harness.app.engine().clone();
        async move {
            engine
                .dispatch(Command::OpenTerminal {
                    panel: Some(PanelId::Left),
                    shell: Some("sh".into()),
                    distro: None,
                    cwd: None,
                    size: None,
                })
                .await
                .expect("opens");
        }
    };
    open(&harness).await;
    harness.settle().await;
    let first = harness
        .app
        .engine()
        .shown_terminal(PanelId::Left)
        .expect("shown");
    let title = harness
        .app
        .engine()
        .panel_terminals(PanelId::Left)
        .into_iter()
        .find(|info| info.terminal == first)
        .expect("listed")
        .title;
    let copy = |terminal, text: String| {
        Message::Terminal(TermMessage::Widget(WidgetEvent::Copy(terminal, text)))
    };
    assert!(matches!(
        harness.app.apply(copy(first, "picked".into()))[..],
        [Effect::Task(_)]
    ));
    assert_eq!(
        harness.app.info.as_deref(),
        Some(crate::i18n::t!("terminal.osc52.copied", title = title.as_str()).as_str())
    );
    let long = "a".repeat(rc_term::osc::MAX_CLIPBOARD + 1);
    assert!(harness.app.apply(copy(first, long)).is_empty());
    // A second terminal in the same panel hides the first.
    open(&harness).await;
    harness.settle().await;
    assert_ne!(
        harness.app.engine().shown_terminal(PanelId::Left),
        Some(first)
    );
    harness.app.info = None;
    assert!(harness.app.apply(copy(first, "hidden".into())).is_empty());
    assert_eq!(harness.app.info, None);
}
