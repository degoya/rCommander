//! The login-script tools end to end, over an in-process pipe like `connections_tests.rs`.

use rc_engine::{Command, CommandClass, ConnectionDraft, LoginScriptDraft, Outcome, Step};
use rmcp::{RoleClient, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::test_support::{Fixture, call, connect};
use crate::tools::class_of as escalate;
use crate::{McpContext, RcMcpServer};

struct Setup {
    _root: TempDir,
    context: McpContext,
}

async fn setup() -> Setup {
    // Headless: these tests drive the confirmation-code path (RC-010-13).
    let built = Fixture::new().with_codes().build().await;
    Setup {
        _root: built.root,
        context: built.context,
    }
}

/// A call, and when it asks for a confirmation the same call with the code: the person agreed.
async fn confirmed(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    let asked = call(client, tool, arguments.clone()).await;
    if asked["confirmation_required"] != true {
        return asked;
    }
    let mut again = arguments;
    again["confirmation"] = asked["confirmation"].clone();
    call(client, tool, again).await
}

fn sudo() -> Value {
    json!([
        { "step": "send_text", "text": "sudo -i", "newline": true },
        { "step": "wait_for", "pattern": "(?i)password.*:" },
        { "step": "send_secret", "source": { "kind": "connection_password" } },
        { "step": "wait_for", "pattern": "#\\s*$", "timeout_ms": 5000 },
    ])
}

fn plain() -> Value {
    json!([
        { "step": "wait_for", "pattern": "\\$\\s*$", "timeout_ms": 5000 },
        { "step": "send_text", "text": "cd /srv", "newline": true },
    ])
}

/// A script with the sudo steps, made in the window.
async fn sudo_by_the_person(context: &McpContext) -> i64 {
    let steps: Vec<Step> = serde_json::from_value(sudo()).expect("steps");
    match context
        .engine()
        .dispatch(Command::CreateLoginScript {
            script: LoginScriptDraft {
                name: "sudo".into(),
                steps,
                step_timeout_ms: None,
                total_timeout_ms: None,
            },
        })
        .await
    {
        Ok(Outcome::LoginScript { script }) => script.id.0,
        other => panic!("a script, got {other:?}"),
    }
}

#[tokio::test]
async fn new_steps_need_a_confirmation_a_rename_does_not() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let arguments = json!({ "name": "srv", "steps": plain() });
    let asked = call(&client, "login_scripts_create", arguments.clone()).await;
    assert_eq!(asked["class"], "exec", "{asked}");
    let created = confirmed(&client, "login_scripts_create", arguments).await;
    assert_eq!(created["name"], "srv", "{created}");
    assert_eq!(created["steps"][1]["newline"], true, "{created}");
    assert_eq!(created["step_timeout_ms"], 15_000);
    assert_eq!(created["secrets"], json!([]));
    let id = created["id"].as_i64().expect("id");

    let empty = call(&client, "login_scripts_create", json!({ "name": "empty" })).await;
    assert_eq!(empty["name"], "empty", "an empty script is write: {empty}");
    let renamed = call(
        &client,
        "login_scripts_update",
        json!({ "id": id, "name": "www", "total_timeout_ms": 30000 }),
    )
    .await;
    assert_eq!(renamed["name"], "www", "{renamed}");

    let steps = json!({ "id": id, "steps": [{ "step": "send_line" }] });
    let asked = call(&client, "login_scripts_update", steps.clone()).await;
    assert_eq!(asked["class"], "exec", "{asked}");
    let updated = confirmed(&client, "login_scripts_update", steps).await;
    assert_eq!(
        updated["steps"],
        json!([{ "step": "send_line" }]),
        "{updated}"
    );
    let listed = call(&client, "login_scripts_list", json!({})).await;
    assert_eq!(listed["scripts"][1], updated, "{listed}");
    let got = call(&client, "login_scripts_get", json!({ "id": id })).await;
    assert_eq!(got, updated);
}

#[tokio::test]
async fn a_send_secret_step_and_its_prompt_are_the_window_s() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let created = confirmed(
        &client,
        "login_scripts_create",
        json!({ "name": "sudo", "steps": sudo() }),
    )
    .await;
    assert_eq!(created["code"], "gui_only", "{created}");

    let id = sudo_by_the_person(&setup.context).await;
    let mut steps = sudo();
    steps[1]["pattern"] = json!("(?i)token:");
    let changed = confirmed(
        &client,
        "login_scripts_update",
        json!({ "id": id, "steps": steps }),
    )
    .await;
    assert_eq!(changed["code"], "gui_only", "{changed}");

    let mut after = sudo();
    after
        .as_array_mut()
        .expect("steps")
        .push(json!({ "step": "send_line" }));
    let appended = confirmed(
        &client,
        "login_scripts_update",
        json!({ "id": id, "steps": after }),
    )
    .await;
    assert_eq!(appended["steps"][4]["step"], "send_line", "{appended}");
}

#[tokio::test]
async fn invalid_steps_and_secrets_are_refused() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let pattern = confirmed(
        &client,
        "login_scripts_create",
        json!({ "name": "x", "steps": [{ "step": "wait_for", "pattern": "(" }] }),
    )
    .await;
    assert_eq!(pattern["code"], "login_script_pattern_invalid", "{pattern}");
    let sleep = confirmed(
        &client,
        "login_scripts_create",
        json!({ "name": "x", "steps": [{ "step": "sleep", "ms": 3_600_000 }] }),
    )
    .await;
    assert_eq!(sleep["code"], "login_script_timeout_invalid", "{sleep}");
    let secret = call(
        &client,
        "login_scripts_create",
        json!({ "name": "x", "password": "hunter2", "steps": [] }),
    )
    .await;
    assert_eq!(secret["code"], "secret_not_accepted", "{secret}");
    let missing = call(&client, "login_scripts_get", json!({ "id": 9 })).await;
    assert_eq!(missing["code"], "login_script_not_found", "{missing}");
}

#[tokio::test]
async fn deleting_wants_a_code_and_assigning_asks_too() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let created = confirmed(
        &client,
        "login_scripts_create",
        json!({ "name": "srv", "steps": plain() }),
    )
    .await;
    let id = created["id"].clone();
    let Ok(Outcome::Connection { connection }) = setup
        .context
        .engine()
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft::new("web", "example.org"),
        })
        .await
    else {
        panic!("a connection");
    };

    let sudo = sudo_by_the_person(&setup.context).await;
    let refused = confirmed(
        &client,
        "login_scripts_assign",
        json!({ "connection_id": connection.id.0, "script_id": sudo }),
    )
    .await;
    assert_eq!(refused["code"], "gui_only", "{refused}");

    let arguments = json!({ "connection_id": connection.id.0, "script_id": id });
    let asked = call(&client, "login_scripts_assign", arguments.clone()).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "exec");
    let assigned = confirmed(&client, "login_scripts_assign", arguments).await;
    assert_eq!(assigned["login_script_id"], id, "{assigned}");

    let asked = call(&client, "login_scripts_delete", json!({ "id": id })).await;
    assert_eq!(asked["class"], "destructive", "{asked}");
    let done = confirmed(&client, "login_scripts_delete", json!({ "id": id })).await;
    assert_eq!(
        done,
        json!({ "deleted": id, "secrets": 0, "connections": 1 }),
        "{done}"
    );
}

#[test]
fn only_new_steps_escalate_to_exec() {
    let write = CommandClass::Write;
    let object = |value: Value| value.as_object().cloned().expect("object");
    // The arguments as the tools take them (a create needs its name, an update its id): the
    // class comes from the command they read into (RC-080-42).
    let steps = object(json!({ "name": "x", "steps": [{ "step": "send_line" }] }));
    let empty = object(json!({ "name": "x", "steps": [] }));
    let new_steps = object(json!({ "id": 1, "steps": [{ "step": "send_line" }] }));
    let cleared = object(json!({ "id": 1, "steps": [] }));
    let none = object(json!({ "id": 1, "name": "x" }));
    assert_eq!(
        escalate("login_scripts_create", Some(&steps), write),
        CommandClass::Exec
    );
    assert_eq!(escalate("login_scripts_create", Some(&empty), write), write);
    assert_eq!(
        escalate("login_scripts_update", Some(&cleared), write),
        CommandClass::Exec
    );
    assert_eq!(escalate("login_scripts_update", Some(&none), write), write);
    assert_eq!(escalate("login_scripts_update", None, write), write);
    assert_eq!(escalate("connections_update", Some(&steps), write), write);
    assert_eq!(
        crate::tools::class_of("login_scripts_update", Some(&new_steps), write),
        CommandClass::Exec
    );
}

#[test]
fn no_tool_stores_or_removes_a_script_secret() {
    let names = RcMcpServer::tool_names();
    let ours: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with("login_scripts_"))
        .collect();
    assert_eq!(
        ours,
        [
            "login_scripts_assign",
            "login_scripts_create",
            "login_scripts_delete",
            "login_scripts_get",
            "login_scripts_list",
            "login_scripts_update",
        ]
    );
}
