//! RC-110-21: the question about `workspaces_apply` names the workspace and where its panels
//! go, and the call runs only on the workspace the question showed.

use std::{path::Path, time::Duration};

use rc_engine::{
    Command, Engine, EngineEvent, McpAnswer, McpQuestion, UpdateWorkspaceRequest, WorkspaceSpec,
};
use serde_json::{Value, json};

use crate::test_support::{Built, Fixture, call};

/// A workspace under `root`: the left panel with the tabs `left`, the right one with the tab
/// `a` and, with `terminal`, a local shell starting there in place of its file list.
fn spec(root: &Path, left: &[&str], terminal: bool) -> Value {
    let tab = |name: &str| json!({ "location": { "backend": "local", "path": root.join(name) } });
    let tabs: Vec<Value> = left.iter().map(|name| tab(name)).collect();
    let mut spec = json!({
        "version": 1,
        "active": "left",
        "left": { "tabs": tabs, "active_tab": 0 },
        "right": { "tabs": [tab("a")], "active_tab": 0 }
    });
    if terminal {
        spec["right"]["view"] = json!("terminal");
        spec["right"]["terminal"] = json!({ "kind": "local", "cwd": root.join("a") });
    }
    spec
}

async fn built(codes: bool) -> Built {
    let fixture = Fixture::new();
    let fixture = if codes { fixture.with_codes() } else { fixture };
    let built = fixture.build().await;
    for name in ["a", "b"] {
        std::fs::create_dir(built.root.path().join(name)).expect("mkdir");
    }
    built
}

/// The value the question shows under `name`; `None` when it shows no such argument.
fn shown(arguments: &Value, name: &str) -> Option<String> {
    arguments
        .as_array()?
        .iter()
        .find(|argument| argument["name"] == name)
        .and_then(|argument| argument["value"].as_str())
        .map(str::to_owned)
}

#[tokio::test]
async fn the_question_names_the_workspace_and_its_places() {
    let built = built(true).await;
    let root = built.root.path();
    let client = built.client().await;
    let created = call(
        &client,
        "workspaces_create",
        json!({ "name": "Both", "spec": spec(root, &["a", "b"], true) }),
    )
    .await;
    let id = created["id"].as_i64().expect("an id");

    // A `workspace` the call brings itself does not reach the person.
    let asked = call(
        &client,
        "workspaces_apply",
        json!({ "id": id, "workspace": "Harmless", "left_terminal": "none" }),
    )
    .await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    let arguments = &asked["arguments"];
    let folder = |name: &str| root.join(name).display().to_string();
    assert_eq!(shown(arguments, "id"), Some(id.to_string()));
    assert_eq!(shown(arguments, "workspace").as_deref(), Some("Both"));
    assert_eq!(
        shown(arguments, "left_tabs"),
        Some(format!("{} | {}", folder("a"), folder("b")))
    );
    assert_eq!(shown(arguments, "right_tabs"), Some(folder("a")));
    assert_eq!(
        shown(arguments, "right_terminal"),
        Some(format!("local: in {}", folder("a")))
    );
    assert_eq!(shown(arguments, "left_terminal"), None, "{arguments}");
    assert_eq!(built.engine.active_workspace(), None, "nothing ran yet");
}

#[tokio::test]
async fn a_code_for_other_places_is_not_redeemed() {
    let built = built(true).await;
    let root = built.root.path();
    let client = built.client().await;
    let created = call(
        &client,
        "workspaces_create",
        json!({ "name": "Both", "spec": spec(root, &["a"], false) }),
    )
    .await;
    let id = created["id"].as_i64().expect("an id");
    let asked = call(&client, "workspaces_apply", json!({ "id": id })).await;
    let code = asked["confirmation"].as_str().expect("a code").to_owned();

    // The workspace changes behind the question: the code was for the places shown.
    let updated = call(
        &client,
        "workspaces_update",
        json!({ "id": id, "spec": spec(root, &["a", "b"], false) }),
    )
    .await;
    assert_eq!(updated["answer"], "done", "{updated}");
    let again = call(
        &client,
        "workspaces_apply",
        json!({ "id": id, "confirmation": code }),
    )
    .await;
    assert_eq!(again["confirmation_required"], true, "asked anew: {again}");
    assert_eq!(built.engine.active_workspace(), None);
    let folder = |name: &str| root.join(name).display().to_string();
    assert_eq!(
        shown(&again["arguments"], "left_tabs"),
        Some(format!("{} | {}", folder("a"), folder("b")))
    );

    // The code for the workspace as it is runs it.
    let code = again["confirmation"].as_str().expect("a code").to_owned();
    let applied = call(
        &client,
        "workspaces_apply",
        json!({ "id": id, "confirmation": code }),
    )
    .await;
    assert_eq!(applied["answer"], "applied", "{applied}");
    assert_eq!(built.engine.active_workspace(), Some(id));
}

/// The person: before allowing the next question, the workspace `id` gets the spec `change`
/// when one is given - as the agent's `workspaces_update` would while the question stands.
fn person_allows(
    engine: &Engine,
    id: i64,
    change: Option<Value>,
) -> tokio::task::JoinHandle<McpQuestion> {
    let mut events = engine.subscribe();
    let engine = engine.clone();
    tokio::spawn(async move {
        loop {
            if let EngineEvent::McpQuestion { question } = events.recv().await.expect("event") {
                if let Some(change) = change {
                    let spec = WorkspaceSpec::from_value(change).expect("reads");
                    engine
                        .dispatch(Command::UpdateWorkspace(UpdateWorkspaceRequest {
                            id,
                            name: None,
                            spec: Some(spec),
                            theme_light: None,
                            theme_dark: None,
                        }))
                        .await
                        .expect("the workspace changes");
                }
                engine
                    .dispatch(Command::AnswerMcpQuestion {
                        question: question.id,
                        answer: McpAnswer::Allow,
                    })
                    .await
                    .expect("the window answers");
                return *question;
            }
        }
    })
}

#[tokio::test]
async fn the_window_shows_the_places_and_a_workspace_changed_meanwhile_does_not_run() {
    let mut built = built(false).await;
    built.context.answer_within(Duration::from_secs(5));
    built.engine.attach_window();
    let root = built.root.path();
    let client = built.client().await;
    let created = call(
        &client,
        "workspaces_create",
        json!({ "name": "Both", "spec": spec(root, &["a"], false) }),
    )
    .await;
    let id = created["id"].as_i64().expect("an id");

    let person = person_allows(&built.engine, id, Some(spec(root, &["a", "b"], false)));
    let refused = call(&client, "workspaces_apply", json!({ "id": id })).await;
    assert_eq!(refused["code"], "workspace_changed", "{refused}");
    assert_eq!(built.engine.active_workspace(), None);
    let question = person.await.expect("joins");
    let value = |name: &str| {
        question
            .arguments
            .iter()
            .find(|argument| argument.name == name)
            .map(|argument| argument.value.clone())
    };
    assert_eq!(value("workspace").as_deref(), Some("Both"));
    assert_eq!(
        value("left_tabs"),
        Some(root.join("a").display().to_string())
    );

    // Unchanged, the allowed call runs.
    let person = person_allows(&built.engine, id, None);
    let applied = call(&client, "workspaces_apply", json!({ "id": id })).await;
    assert_eq!(applied["answer"], "applied", "{applied}");
    assert_eq!(built.engine.active_workspace(), Some(id));
    person.await.expect("joins");
}

#[tokio::test]
async fn a_workspace_that_is_not_there_is_refused_before_anyone_is_asked() {
    let built = built(false).await;
    built.engine.attach_window();
    let client = built.client().await;
    let gone = call(&client, "workspaces_apply", json!({ "id": 999 })).await;
    assert_eq!(gone["code"], "workspace_unknown", "{gone}");
    assert!(built.engine.mcp_questions().is_empty(), "nobody was asked");
}
