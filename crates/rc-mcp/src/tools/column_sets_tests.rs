//! The column set tools over an in-process pipe: saved, read, changed, ordered, shown and
//! deleted, each tool in the class of its command, a spec that does not fit refused with its
//! code.

use serde_json::{Value, json};

use crate::test_support::{Fixture, call, connect};

fn code_of(asked: &Value) -> String {
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    asked["confirmation"].as_str().expect("code").to_owned()
}

fn spec() -> Value {
    json!({
        "version": 1,
        "columns": [
            { "field": "name", "width": 240 },
            { "field": "created", "width": 128, "title": "Made" },
            { "field": "size", "width": 96, "align": "right" }
        ],
        "masks": ["D:\\Photos\\*"]
    })
}

#[tokio::test]
async fn a_column_set_is_saved_read_changed_shown_and_deleted() {
    let built = Fixture::new().with_codes().build().await;
    let client = connect(&built.context).await;
    let created = call(
        &client,
        "column_sets_create",
        json!({ "name": "Photos", "spec": spec() }),
    )
    .await;
    assert_eq!(created["kind"], "column_sets", "{created}");
    assert_eq!(created["answer"], "saved", "{created}");
    let id = created["id"].as_i64().expect("an id");

    let listed = call(&client, "column_sets_list", json!({})).await;
    assert_eq!(listed["sets"][0]["name"], "Photos", "{listed}");
    assert_eq!(listed["sets"][0]["spec"]["columns"][1]["title"], "Made");

    let mut changed = spec();
    changed["masks"] = json!([]);
    let updated = call(
        &client,
        "column_sets_update",
        json!({ "id": id, "spec": changed }),
    )
    .await;
    assert_eq!(updated["answer"], "done", "{updated}");
    let renamed = call(
        &client,
        "column_sets_rename",
        json!({ "id": id, "name": "Pictures" }),
    )
    .await;
    assert_eq!(renamed["answer"], "done", "{renamed}");
    let moved = call(
        &client,
        "column_sets_move",
        json!({ "id": id, "position": 3 }),
    )
    .await;
    assert_eq!(moved["answer"], "done", "{moved}");
    let read = call(&client, "column_sets_get", json!({ "id": id })).await;
    assert_eq!(read["name"], "Pictures", "{read}");
    assert_eq!(read["spec"].get("masks"), None, "no masks are left out");

    let shown = call(
        &client,
        "panel_set_view",
        json!({ "mode": "custom", "set": id }),
    )
    .await;
    assert_eq!(shown["mode"]["custom"]["set"], id, "{shown}");
    let comments = call(&client, "panel_set_view", json!({ "mode": "comments" })).await;
    assert_eq!(comments["mode"], "comments", "{comments}");
    let missing = call(&client, "panel_set_view", json!({ "mode": "custom" })).await;
    assert_eq!(missing["code"], "invalid_params", "{missing}");

    let asked = call(&client, "column_sets_delete", json!({ "id": id })).await;
    assert_eq!(asked["class"], "destructive", "{asked}");
    let code = code_of(&asked);
    let deleted = call(
        &client,
        "column_sets_delete",
        json!({ "id": id, "confirmation": code }),
    )
    .await;
    assert_eq!(deleted["answer"], "done", "{deleted}");
    let gone = call(&client, "column_sets_get", json!({ "id": id })).await;
    assert_eq!(gone["code"], "column_set_unknown", "{gone}");
    let unknown = call(
        &client,
        "panel_set_view",
        json!({ "mode": "custom", "set": id }),
    )
    .await;
    assert_eq!(unknown["code"], "column_set_unknown", "{unknown}");
}

#[tokio::test]
async fn a_spec_that_does_not_fit_is_refused_with_its_code() {
    let built = Fixture::new().build().await;
    let client = connect(&built.context).await;
    let future = call(
        &client,
        "column_sets_create",
        json!({ "name": "Next", "spec": { "version": 2 } }),
    )
    .await;
    assert_eq!(future["code"], "column_set_version", "{future}");
    let mut wide = spec();
    wide["columns"][0]["width"] = json!(2000);
    let refused = call(
        &client,
        "column_sets_create",
        json!({ "name": "Wide", "spec": wide }),
    )
    .await;
    assert_eq!(refused["code"], "column_set_invalid", "{refused}");
    let unnamed = call(
        &client,
        "column_sets_create",
        json!({ "name": " ", "spec": spec() }),
    )
    .await;
    assert_eq!(unnamed["code"], "column_set_name", "{unnamed}");
}

#[test]
fn every_column_set_command_has_its_tool() {
    let names = crate::RcMcpServer::tool_names();
    for tool in [
        "column_sets_list",
        "column_sets_get",
        "column_sets_create",
        "column_sets_update",
        "column_sets_rename",
        "column_sets_move",
        "column_sets_delete",
    ] {
        assert!(names.contains(&tool.to_owned()), "{tool}");
    }
}
