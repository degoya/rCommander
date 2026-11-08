//! The button bars' vocabulary: the bars of the window and their settings, the codes, the serde
//! forms the window, MCP and the database read.

use super::{BarKind, BarPlace, ButtonBarError, ButtonBarsOutcome, default_spec};
use crate::{
    EngineError, UserCommandError,
    settings::{SettingKind, setting},
};

#[test]
fn every_bar_of_the_window_is_a_switch_of_the_schema() {
    for bar in BarKind::ALL {
        let spec = setting(bar.setting_key()).expect("in the schema");
        assert!(
            matches!(spec.kind, SettingKind::Bool { .. }),
            "{}",
            bar.as_str()
        );
        assert!(
            !spec.protected,
            "{}: the View menu switches it",
            bar.as_str()
        );
        let text = serde_json::to_value(bar).expect("writes");
        assert_eq!(text, bar.as_str());
    }
}

#[test]
fn the_button_bar_is_on_and_the_vertical_one_off_at_the_first_start() {
    let shown = |bar: BarKind| match setting(bar.setting_key()).map(|spec| spec.kind) {
        Some(SettingKind::Bool { default }) => default,
        other => panic!("a switch: {other:?}"),
    };
    assert!(shown(BarKind::ButtonBar), "owner 2026-11-07");
    assert!(!shown(BarKind::VerticalButtonBar));
    assert!(shown(BarKind::DriveBar));
    assert!(shown(BarKind::CommandLine));
    assert!(shown(BarKind::StatusBar));
}

#[test]
fn every_code_is_listed_once_and_carried_by_the_engine_error() {
    let mut codes = ButtonBarError::ALL_CODES.to_vec();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), ButtonBarError::ALL_CODES.len());
    let error: EngineError = ButtonBarError::Cycle.into();
    assert_eq!(error.code(), "button_bar_cycle");
    assert!(matches!(
        error,
        EngineError::UserCommand(UserCommandError::ButtonBar(ButtonBarError::Cycle))
    ));
    for code in ButtonBarError::ALL_CODES {
        assert!(
            !UserCommandError::ALL_CODES.contains(code),
            "{code} is the button bars' own"
        );
    }
}

#[test]
fn the_places_and_answers_have_their_serde_names() {
    assert_eq!(
        serde_json::to_value(BarPlace::Vertical).expect("writes"),
        "vertical"
    );
    let outcome = serde_json::to_value(ButtonBarsOutcome::Ran {
        started: 1,
        asking: false,
    })
    .expect("writes");
    assert_eq!(outcome["answer"], "ran");
    let spec = serde_json::to_value(default_spec()).expect("writes");
    assert_eq!(spec["version"], 1);
    assert_eq!(spec["items"][0]["type"], "button");
    assert_eq!(spec["items"][0]["action"]["command"]["kind"], "internal");
    assert_eq!(spec["items"][1]["type"], "separator");
}
