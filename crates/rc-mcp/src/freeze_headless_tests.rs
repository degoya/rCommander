//! RC-080-55: the freeze holds without a question in the window. A headless code runs the call
//! it was issued for - what the panel held then - and a call allowed for the session runs as the
//! panels stood when it came, pinned; a panel moved elsewhere refuses with `panel_changed`.

use std::{fs, time::Duration};

use rc_engine::{Command, CommandClass, EngineError, PanelId};
use serde_json::{Value, json};

use crate::ask::{Gate, ask};
use crate::gate_settings::GateSettings;
use crate::test_support::{Built, Fixture, call};

/// `root/` with `a.txt`, `b.txt`, `c.txt` and the folders `docs/` and `other/`; both panels on
/// `root`, no window, `a.txt` selected; confirmation codes allowed.
async fn setup() -> Built {
    let fixture = Fixture::new().with_codes();
    let root = fixture.root();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).expect("write");
    }
    fs::create_dir(root.join("docs")).expect("mkdir");
    fs::create_dir(root.join("other")).expect("mkdir");
    let built = fixture.build().await;
    built
        .engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: vec!["a.txt".to_owned()],
        })
        .await
        .expect("selects");
    built
}

/// The value the code's answer names for `argument`.
fn named<'a>(asked: &'a Value, argument: &str) -> Option<&'a str> {
    asked["arguments"]
        .as_array()?
        .iter()
        .find(|shown| shown["name"] == argument)?
        .get("value")?
        .as_str()
}

#[tokio::test]
async fn a_code_deletes_what_its_question_named_not_what_is_selected_then() {
    let built = setup().await;
    let agent = built.client().await;

    let asked = call(&agent, "ops_delete", json!({ "permanent": true })).await;
    let code = asked["confirmation"]
        .as_str()
        .unwrap_or_else(|| panic!("a code: {asked}"))
        .to_owned();
    let sources = named(&asked, "sources").unwrap_or_else(|| panic!("named: {asked}"));
    assert!(
        sources.contains("a.txt") && !sources.contains("b.txt"),
        "{asked}"
    );
    assert_eq!(named(&asked, "panel"), Some("left"), "{asked}");

    // Between the code and its use the same agent selects everything.
    let all = call(&agent, "panel_select", json!({ "action": "all" })).await;
    assert!(all.get("code").is_none(), "{all}");

    let queued = call(
        &agent,
        "ops_delete",
        json!({ "permanent": true, "confirmation": code }),
    )
    .await;
    let id = queued["id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a job: {queued}"));
    tokio::time::timeout(Duration::from_secs(10), built.engine.ops().wait(id))
        .await
        .expect("in time")
        .expect("known");
    let root = built.root.path();
    assert!(!root.join("a.txt").exists(), "the named entry is deleted");
    for kept in ["b.txt", "c.txt", "docs", "other"] {
        assert!(root.join(kept).exists(), "{kept} was not named and stays");
    }
}

#[tokio::test]
async fn a_code_whose_panel_moved_refuses_with_panel_changed() {
    let built = setup().await;
    let agent = built.client().await;

    let asked = call(&agent, "ops_delete", json!({ "permanent": true })).await;
    let code = asked["confirmation"]
        .as_str()
        .unwrap_or_else(|| panic!("a code: {asked}"))
        .to_owned();
    let moved = call(&agent, "navigate", json!({ "path": "docs" })).await;
    assert!(moved.get("code").is_none(), "{moved}");

    let refused = call(
        &agent,
        "ops_delete",
        json!({ "permanent": true, "confirmation": code }),
    )
    .await;
    assert_eq!(refused["code"], "panel_changed", "{refused}");
    assert!(built.root.path().join("a.txt").exists(), "nothing deleted");
}

#[tokio::test]
async fn a_code_answers_only_the_call_it_was_issued_for() {
    let built = setup().await;
    let agent = built.client().await;
    let asked = call(&agent, "ops_delete", json!({ "permanent": true })).await;
    let code = asked["confirmation"].as_str().expect("a code").to_owned();
    // Other sources than the frozen ones: a new question, not the old call.
    let other = call(
        &agent,
        "ops_delete",
        json!({ "permanent": true, "sources": ["b.txt"], "confirmation": code }),
    )
    .await;
    assert_eq!(other["confirmation_required"], true, "{other}");
    assert!(built.root.path().join("b.txt").exists());
}

/// A copy allowed for the session takes the selection as it stands when the call comes, as
/// `sources`, and runs with both panels pinned: one moved before it runs refuses it.
#[tokio::test]
async fn a_call_allowed_for_the_session_runs_frozen_and_pinned() {
    let built = setup().await;
    let engine = &built.engine;
    let settings = GateSettings::load(&built.db).await.expect("settings");
    engine.allow_mcp_for_session("s1", "ops_copy", None, None);

    let gate = ask(
        &built.context,
        &settings,
        "s1",
        "ops_copy",
        CommandClass::Write,
        json!({}).as_object(),
    )
    .await
    .expect("gates");
    let Gate::Run(Some(frozen)) = gate else {
        panic!("runs frozen: {gate:?}");
    };
    assert_eq!(frozen.arguments["sources"], json!(["a.txt"]));
    assert_eq!(frozen.arguments["panel"], "left");
    assert_eq!(frozen.pins.len(), 2, "its own panel and the target's");

    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "other".to_owned(),
        })
        .await
        .expect("navigates");
    let run = rc_engine::with_mcp_pins(
        frozen.pins,
        engine.dispatch_mcp(Command::ListDir {
            panel: None,
            path: None,
        }),
    )
    .await;
    assert!(
        matches!(run, Err(EngineError::PanelChanged { .. })),
        "{run:?}"
    );
}
