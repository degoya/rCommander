//! RC-080-34 (S2): a call is frozen to what its question shows. The attack end to end - the
//! question waits, the same agent selects everything and moves the cursor, the person allows -
//! runs only what the question showed; a panel moved elsewhere refuses with `panel_changed`.

use std::{fs, path::Path, time::Duration};

use rc_engine::{Command, CursorTarget, Engine, EngineEvent, McpAnswer, McpQuestion, PanelId};
use rmcp::{RoleClient, service::RunningService};
use serde_json::{Value, json};

use super::freeze;
use crate::test_support::{Built, Fixture, call};

/// `root/` with `a.txt`, `b.txt`, `c.txt` and the folders `docs/` (holding its own `a.txt`) and
/// `other/`; both panels on `root`, the window open, `a.txt` selected and the cursor on it.
async fn setup() -> Built {
    let fixture = Fixture::new();
    let root = fixture.root();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(name), name).expect("write");
    }
    fs::create_dir(root.join("docs")).expect("mkdir");
    fs::create_dir(root.join("other")).expect("mkdir");
    fs::write(root.join("docs").join("a.txt"), b"inner").expect("write");
    let mut built = fixture.build().await;
    built.context.answer_within(Duration::from_secs(30));
    built.engine.attach_window();
    built
        .engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: vec!["a.txt".to_owned()],
        })
        .await
        .expect("selects");
    cursor_on(&built.engine, "a.txt").await;
    built
}

async fn cursor_on(engine: &Engine, name: &str) {
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Name(name.to_owned()),
        })
        .await
        .expect("sets the cursor");
}

/// The person: waits for the question, lets the agent `meddle` through a second connection
/// while it waits, then answers `answer`.
async fn person(
    engine: &Engine,
    events: &mut tokio::sync::broadcast::Receiver<EngineEvent>,
    meddler: &RunningService<RoleClient, ()>,
    meddle: &[(&str, Value)],
    answer: McpAnswer,
) -> McpQuestion {
    let question = loop {
        if let EngineEvent::McpQuestion { question } = events.recv().await.expect("event") {
            break *question;
        }
    };
    for (tool, arguments) in meddle {
        let done = call(meddler, tool, arguments.clone()).await;
        assert!(done.get("code").is_none(), "{tool}: {done}");
    }
    engine
        .dispatch(Command::AnswerMcpQuestion {
            question: question.id,
            answer,
        })
        .await
        .expect("the window answers");
    question
}

/// The agent's moves while the question waits: everything selected, the cursor elsewhere.
fn select_all_and_move_to(name: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("panel_select", json!({ "action": "all" })),
        ("set_cursor", json!({ "name": name })),
    ]
}

#[tokio::test]
async fn a_delete_allowed_deletes_what_the_question_showed_not_what_is_selected_then() {
    let built = setup().await;
    let engine = &built.engine;
    let agent = built.client().await;
    let meddler = built.client().await;
    let mut events = engine.subscribe();
    let meddle = select_all_and_move_to("c.txt");

    let (queued, question) = tokio::join!(
        call(&agent, "ops_delete", json!({ "permanent": true })),
        person(engine, &mut events, &meddler, &meddle, McpAnswer::Allow),
    );
    assert!(question.panel.selection, "{question:?}");
    assert_eq!(question.panel.entries, ["a.txt"]);
    let id = queued["id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a job: {queued}"));
    tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(id))
        .await
        .expect("in time")
        .expect("known");

    let root = built.root.path();
    assert!(!root.join("a.txt").exists(), "the shown entry is deleted");
    for kept in ["b.txt", "c.txt", "docs/a.txt"] {
        assert!(root.join(kept).exists(), "{kept} was not shown and stays");
    }
    assert!(root.join("docs").is_dir() && root.join("other").is_dir());
}

#[tokio::test]
async fn an_open_allowed_opens_the_entry_the_question_showed() {
    let built = setup().await;
    let engine = &built.engine;
    cursor_on(engine, "docs").await;
    let start = engine.panel(PanelId::Left).path.path;
    let agent = built.client().await;
    let meddler = built.client().await;
    let mut events = engine.subscribe();
    let meddle = select_all_and_move_to("other");

    let (opened, question) = tokio::join!(
        call(&agent, "file_open", json!({})),
        person(engine, &mut events, &meddler, &meddle, McpAnswer::Allow),
    );
    assert!(
        !question.panel.selection,
        "the cursor's entry: {question:?}"
    );
    assert_eq!(question.panel.entries, ["docs"]);
    assert!(
        question
            .arguments
            .iter()
            .any(|argument| argument.name == "path" && argument.value == "docs"),
        "{question:?}"
    );
    assert!(opened.get("code").is_none(), "{opened}");
    let shown = engine.panel(PanelId::Left).path.path;
    assert_eq!(shown, start.join("docs"), "{opened}");
}

#[tokio::test]
async fn a_panel_moved_while_the_question_waited_refuses_with_panel_changed() {
    let built = setup().await;
    let engine = &built.engine;
    let agent = built.client().await;
    let meddler = built.client().await;
    let mut events = engine.subscribe();
    let meddle = [("navigate", json!({ "path": "docs" }))];

    let (refused, _) = tokio::join!(
        call(
            &agent,
            "ops_delete",
            json!({ "sources": ["a.txt"], "permanent": true })
        ),
        person(engine, &mut events, &meddler, &meddle, McpAnswer::Allow),
    );
    assert_eq!(refused["code"], "panel_changed", "{refused}");
    let root = built.root.path();
    assert!(root.join("a.txt").exists() && root.join("docs/a.txt").exists());

    // The other panel a copy goes to is pinned as well.
    let back = call(
        &meddler,
        "navigate",
        json!({ "path": "..", "panel": "left" }),
    )
    .await;
    assert!(back.get("code").is_none(), "{back}");
    let meddle = [("navigate", json!({ "path": "other", "panel": "right" }))];
    let (refused, question) = tokio::join!(
        call(
            &agent,
            "ops_copy",
            json!({ "sources": ["b.txt"], "on_conflict": "overwrite" })
        ),
        person(engine, &mut events, &meddler, &meddle, McpAnswer::Allow),
    );
    assert_eq!(refused["code"], "panel_changed", "{refused}");
    assert!(
        question
            .arguments
            .iter()
            .any(|argument| argument.name == "right_panel"),
        "the target folder is shown: {question:?}"
    );
    assert!(!root.join("other/b.txt").exists(), "nothing copied");
}

#[tokio::test]
async fn freezing_writes_out_the_panel_and_what_it_takes() {
    let built = setup().await;
    let engine = &built.engine;
    let arguments = |value: Value| value.as_object().cloned();

    let frozen = freeze(engine, "ops_delete", arguments(json!({})).as_ref())
        .expect("freezes")
        .expect("ops_delete reads the panel");
    assert_eq!(frozen.arguments["panel"], "left");
    assert_eq!(frozen.arguments["sources"], json!(["a.txt"]));
    assert!(
        frozen.shown.get("sources").is_none(),
        "shown as the panel's entries"
    );
    assert_eq!(frozen.pins.len(), 1);
    assert_eq!(frozen.pins[0].path, engine.panel(PanelId::Left).path);

    let named = json!({ "sources": ["b.txt"], "panel": "right" });
    let frozen = freeze(engine, "ops_copy", arguments(named).as_ref())
        .expect("freezes")
        .expect("ops_copy reads the panels");
    assert_eq!(frozen.arguments["sources"], json!(["b.txt"]), "as called");
    assert_eq!(frozen.arguments["panel"], "right");
    assert_eq!(frozen.shown["sources"], json!(["b.txt"]));
    let left = engine.panel(PanelId::Left).path;
    assert_eq!(
        frozen.shown["left_panel"],
        json!(left.path.display().to_string())
    );
    assert_eq!(frozen.pins.len(), 2, "its own panel and the target's");

    let frozen = freeze(engine, "file_open", None)
        .expect("freezes")
        .expect("file_open reads the cursor");
    assert_eq!(frozen.arguments["path"], "a.txt");
    assert_eq!(frozen.shown["path"], "a.txt");

    for tool in ["terminal_send_input", "ssh_exec", "navigate"] {
        assert!(
            freeze(engine, tool, None).expect("no refusal").is_none(),
            "{tool}"
        );
    }
    let wrong = freeze(
        engine,
        "ops_delete",
        arguments(json!({ "panel": "middle" })).as_ref(),
    );
    assert!(wrong.is_err(), "an unknown panel is refused");
}

#[tokio::test]
async fn a_call_that_would_take_names_from_an_empty_panel_is_refused() {
    let built = setup().await;
    let engine = &built.engine;
    let empty = tempfile::tempdir().expect("temp dir");
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: empty.path().display().to_string(),
        })
        .await
        .expect("navigates");
    let refused = freeze(engine, "ops_delete", None).expect_err("nothing to take");
    assert_eq!(refused.is_error, Some(true));
    let refused = freeze(engine, "file_open", None).expect_err("no cursor entry");
    assert_eq!(refused.is_error, Some(true));
    assert!(Path::new(empty.path()).is_dir());
}
