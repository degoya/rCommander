//! The workspace tools over an in-process pipe: saved, read, changed, ordered, applied and deleted,
//! each tool in the class of its command, and no tool for the workspaces' window.

use serde_json::{Value, json};

use crate::test_support::{Fixture, call, connect};

fn code_of(asked: &Value) -> String {
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    asked["confirmation"].as_str().expect("code").to_owned()
}

fn spec(root: &std::path::Path) -> Value {
    let tab = |name: &str| json!({ "location": { "backend": "local", "path": root.join(name) } });
    json!({
        "version": 1,
        "active": "right",
        "split_permille": 300,
        "left": { "tabs": [tab("a"), tab("b")], "active_tab": 1 },
        "right": { "tabs": [tab("a")], "active_tab": 0 }
    })
}

#[tokio::test]
async fn a_workspace_is_saved_read_changed_applied_and_deleted() {
    let built = Fixture::new().with_codes().build().await;
    for name in ["a", "b"] {
        std::fs::create_dir(built.root.path().join(name)).expect("mkdir");
    }
    let client = connect(&built.context).await;
    let created = call(
        &client,
        "workspaces_create",
        json!({ "name": "Both", "spec": spec(built.root.path()), "theme_dark": "fluent-dark" }),
    )
    .await;
    assert_eq!(created["kind"], "workspaces", "{created}");
    assert_eq!(created["answer"], "saved", "{created}");
    let id = created["id"].as_i64().expect("an id");

    let listed = call(&client, "workspaces_list", json!({})).await;
    assert_eq!(listed["workspaces"][0]["name"], "Both", "{listed}");
    assert_eq!(listed["workspaces"][0]["theme_dark"], "fluent-dark");
    assert_eq!(listed["workspaces"][0]["active"], false);

    let renamed = call(
        &client,
        "workspaces_rename",
        json!({ "id": id, "name": "Two" }),
    )
    .await;
    assert_eq!(renamed["answer"], "done", "{renamed}");
    let updated = call(
        &client,
        "workspaces_update",
        json!({ "id": id, "theme_dark": "" }),
    )
    .await;
    assert_eq!(updated["answer"], "done", "{updated}");
    let read = call(&client, "workspaces_get", json!({ "id": id })).await;
    assert_eq!(read["name"], "Two", "{read}");
    assert_eq!(
        read["theme_dark"],
        Value::Null,
        "an empty theme leaves the window's"
    );
    assert_eq!(read["spec"]["split_permille"], 300);

    let asked = call(&client, "workspaces_apply", json!({ "id": id })).await;
    assert_eq!(asked["class"], "exec", "it may start shells: {asked}");
    let code = code_of(&asked);
    let applied = call(
        &client,
        "workspaces_apply",
        json!({ "id": id, "confirmation": code }),
    )
    .await;
    assert_eq!(applied["answer"], "applied", "{applied}");
    assert_eq!(applied["skipped"], json!([]));
    assert_eq!(built.engine.active_workspace(), Some(id));

    let asked = call(&client, "workspaces_delete", json!({ "id": id })).await;
    assert_eq!(asked["class"], "destructive", "{asked}");
    let code = code_of(&asked);
    let deleted = call(
        &client,
        "workspaces_delete",
        json!({ "id": id, "confirmation": code }),
    )
    .await;
    assert_eq!(deleted["answer"], "done", "{deleted}");
    let gone = call(&client, "workspaces_get", json!({ "id": id })).await;
    assert_eq!(gone["code"], "workspace_unknown", "{gone}");
}

#[tokio::test]
async fn a_spec_that_does_not_fit_is_refused_with_its_code() {
    let built = Fixture::new().build().await;
    let client = connect(&built.context).await;
    let future = call(
        &client,
        "workspaces_create",
        json!({ "name": "Next", "spec": { "version": 2 } }),
    )
    .await;
    assert_eq!(future["code"], "workspace_version", "{future}");
    let mut archive = spec(built.root.path());
    archive["left"]["tabs"] = json!([]);
    let empty = call(
        &client,
        "workspaces_create",
        json!({ "name": "Empty", "spec": archive }),
    )
    .await;
    assert_eq!(empty["code"], "workspace_tabs", "{empty}");
    let unnamed = call(&client, "workspaces_capture", json!({})).await;
    assert_eq!(unnamed["code"], "workspace_name", "{unnamed}");
    let captured = call(&client, "workspaces_capture", json!({ "name": "Now" })).await;
    assert_eq!(captured["answer"], "saved", "{captured}");
    let second = call(&client, "workspaces_capture", json!({ "name": "Later" })).await;
    let moved = call(
        &client,
        "workspaces_move",
        json!({ "id": second["id"], "position": 0 }),
    )
    .await;
    assert_eq!(moved["answer"], "done", "{moved}");
    let listed = call(&client, "workspaces_list", json!({})).await;
    assert_eq!(listed["workspaces"][0]["name"], "Later", "{listed}");
}

/// The workspaces' window is the person's: no tool opens it.
#[test]
fn no_tool_opens_the_workspaces_window() {
    let names = crate::RcMcpServer::tool_names();
    for tool in [
        "workspaces_list",
        "workspaces_get",
        "workspaces_create",
        "workspaces_capture",
        "workspaces_update",
        "workspaces_rename",
        "workspaces_move",
        "workspaces_delete",
        "workspaces_apply",
    ] {
        assert!(names.contains(&tool.to_owned()), "{tool}");
    }
    assert!(!names.contains(&"workspaces_manager".to_owned()));
}
