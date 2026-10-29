//! The connection tools end to end, over an in-process pipe like `server_tests.rs`.

use rc_engine::{Command, Outcome};
use rmcp::{RoleClient, ServiceExt as _, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::refuse_secrets;
use crate::test_support::Fixture;
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

async fn connect(context: &McpContext) -> RunningService<RoleClient, ()> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    let server = RcMcpServer::new(context.clone());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_side).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_side).await.expect("client")
}

async fn call(client: &RunningService<RoleClient, ()>, tool: &str, arguments: Value) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .expect("call");
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

/// How many connections the engine holds, read past MCP.
async fn connection_count(context: &McpContext) -> usize {
    fn count(nodes: &[rc_engine::FolderNode]) -> usize {
        nodes
            .iter()
            .map(|node| node.connections.len() + count(&node.folders))
            .sum()
    }
    match context
        .engine()
        .dispatch(Command::ListConnections)
        .await
        .expect("list")
    {
        Outcome::ConnectionTree { tree } => tree.connections.len() + count(&tree.folders),
        other => panic!("the tree, got {other:?}"),
    }
}

#[tokio::test]
async fn an_agent_builds_and_reads_the_tree() {
    let setup = setup().await;
    let client = connect(&setup.context).await;

    let folder = call(
        &client,
        "connections_folder_create",
        json!({ "name": "prod" }),
    )
    .await;
    let folder_id = folder["id"].as_i64().expect("folder id");
    let created = call(
        &client,
        "connections_create",
        json!({
            "name": "web",
            "host": "web.example.org",
            "username": "deploy",
            "auth_method": "agent",
            "default_view": "terminal",
            "folder_id": folder_id,
        }),
    )
    .await;
    assert_eq!(created["port"], 22, "{created}");
    assert_eq!(created["auth_method"], "agent");
    assert_eq!(created["kind"], "ssh");
    let id = created["id"].as_i64().expect("id");

    let tree = call(&client, "connections_list", json!({})).await;
    assert_eq!(tree["folders"][0]["name"], "prod", "{tree}");
    assert_eq!(
        tree["folders"][0]["connections"][0]["host"],
        "web.example.org"
    );

    let updated = call(
        &client,
        "connections_update",
        json!({ "id": id, "port": 2222, "username": "" }),
    )
    .await;
    assert_eq!(updated["port"], 2222, "{updated}");
    assert_eq!(updated["username"], Value::Null);

    let moved = call(&client, "connections_move", json!({ "id": id })).await;
    assert_eq!(moved["folder_id"], Value::Null, "{moved}");
    let got = call(&client, "connections_get", json!({ "id": id })).await;
    assert_eq!(got["name"], "web");
}

#[tokio::test]
async fn invalid_input_answers_the_same_codes_as_the_engine() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let empty = call(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "" }),
    )
    .await;
    assert_eq!(empty["code"], "host_empty", "{empty}");
    let port = call(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "h", "port": 0 }),
    )
    .await;
    assert_eq!(port["code"], "port_invalid", "{port}");
    call(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "h" }),
    )
    .await;
    let taken = call(
        &client,
        "connections_create",
        json!({ "name": "Web", "host": "h" }),
    )
    .await;
    assert_eq!(taken["code"], "name_taken", "{taken}");
    let top = call(&client, "connections_folder_create", json!({ "name": "a" })).await;
    let inner = call(
        &client,
        "connections_folder_create",
        json!({ "name": "b", "parent_id": top["id"] }),
    )
    .await;
    let cycle = call(
        &client,
        "connections_folder_move",
        json!({ "id": top["id"], "parent_id": inner["id"] }),
    )
    .await;
    assert_eq!(cycle["code"], "folder_cycle", "{cycle}");
}

#[tokio::test]
async fn a_password_in_a_call_is_refused_not_ignored() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let refused = call(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "h", "password": "hunter2-canary" }),
    )
    .await;
    assert_eq!(refused["code"], "secret_not_accepted", "{refused}");
    assert!(
        !refused.to_string().contains("hunter2-canary"),
        "the value is never echoed: {refused}"
    );
    assert_eq!(connection_count(&setup.context).await, 0);

    let created = call(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "h" }),
    )
    .await;
    for (name, value) in [
        ("passphrase", json!("x")),
        ("private_key", json!("-----BEGIN OPENSSH PRIVATE KEY-----")),
        ("Password", json!("x")),
        ("extra", json!({ "password": "x" })),
    ] {
        let mut arguments = json!({ "id": created["id"], "notes": "n" });
        arguments[name] = value;
        let refused = call(&client, "connections_update", arguments).await;
        assert_eq!(refused["code"], "secret_not_accepted", "{name}: {refused}");
    }
    let unchanged = call(&client, "connections_get", json!({ "id": created["id"] })).await;
    assert_eq!(unchanged["notes"], Value::Null, "{unchanged}");
}

#[tokio::test]
async fn connections_delete_without_a_code_deletes_nothing() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let created = call(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "h" }),
    )
    .await;

    let asked = call(
        &client,
        "connections_delete",
        json!({ "id": created["id"] }),
    )
    .await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive");
    assert_eq!(connection_count(&setup.context).await, 1);

    let wrong = call(
        &client,
        "connections_delete",
        json!({ "id": created["id"], "confirmation": "0000" }),
    )
    .await;
    assert_eq!(wrong["confirmation_required"], true, "{wrong}");
    assert_eq!(connection_count(&setup.context).await, 1);

    let code = wrong["confirmation"].as_str().expect("code").to_owned();
    let done = call(
        &client,
        "connections_delete",
        json!({ "id": created["id"], "confirmation": code }),
    )
    .await;
    assert_eq!(done["deleted"]["connection"], created["id"], "{done}");
    assert_eq!(connection_count(&setup.context).await, 0);
}

#[tokio::test]
async fn a_folder_delete_wants_a_code_and_recursive_for_content() {
    let setup = setup().await;
    let client = connect(&setup.context).await;
    let folder = call(
        &client,
        "connections_folder_create",
        json!({ "name": "prod" }),
    )
    .await;
    call(
        &client,
        "connections_create",
        json!({ "name": "web", "host": "h", "folder_id": folder["id"] }),
    )
    .await;

    let asked = call(
        &client,
        "connections_folder_delete",
        json!({ "id": folder["id"] }),
    )
    .await;
    let code = asked["confirmation"].as_str().expect("code").to_owned();
    let not_empty = call(
        &client,
        "connections_folder_delete",
        json!({ "id": folder["id"], "confirmation": code }),
    )
    .await;
    assert_eq!(not_empty["code"], "folder_not_empty", "{not_empty}");

    let asked = call(
        &client,
        "connections_folder_delete",
        json!({ "id": folder["id"], "recursive": true }),
    )
    .await;
    let code = asked["confirmation"].as_str().expect("code").to_owned();
    let done = call(
        &client,
        "connections_folder_delete",
        json!({ "id": folder["id"], "recursive": true, "confirmation": code }),
    )
    .await;
    assert_eq!(done["folders"], 1, "{done}");
    assert_eq!(done["connections"], 1);
    assert_eq!(connection_count(&setup.context).await, 0);
}

fn object(value: Value) -> rmcp::model::JsonObject {
    value.as_object().cloned().expect("object")
}

#[test]
fn only_the_connection_tools_refuse_secret_names() {
    let password = object(json!({ "password": "x" }));
    assert!(refuse_secrets("connections_create", Some(&password)).is_some());
    assert!(refuse_secrets("navigate", Some(&password)).is_none());
    assert!(refuse_secrets("connections_create", None).is_none());
    let harmless = object(json!({
        "name": "web",
        "key_path": "C:\\keys\\id_ed25519",
        "auth_method": "password",
        "confirmation": "abc",
    }));
    assert!(refuse_secrets("connections_create", Some(&harmless)).is_none());
    for name in [
        "key",
        "token",
        "ssh_password",
        "keyPassphrase",
        "client_secret",
    ] {
        let named = object(json!({ name: "x" }));
        assert!(
            refuse_secrets("connections_update", Some(&named)).is_some(),
            "{name}"
        );
    }
    let nested = object(json!({ "list": [{ "passwd": "x" }] }));
    assert!(refuse_secrets("connections_update", Some(&nested)).is_some());
}
