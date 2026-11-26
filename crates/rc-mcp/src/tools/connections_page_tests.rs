//! `connections_list` in pages, end to end over an in-process pipe.

use serde_json::{Value, json};

use crate::test_support::{Fixture, call};

/// The names of the connections in `nodes`, depth first.
fn names(nodes: &Value) -> Vec<String> {
    nodes
        .as_array()
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| node["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn the_tree_comes_in_pages_with_its_folders_around_each() {
    let built = Fixture::new().build().await;
    let client = built.client().await;
    let folder = call(
        &client,
        "connections_folder_create",
        json!({ "name": "prod" }),
    )
    .await;
    let folder_id = folder["id"].as_i64().expect("folder id");
    for name in ["a", "b"] {
        let created = call(
            &client,
            "connections_create",
            json!({ "name": name, "host": "h.example", "folder_id": folder_id }),
        )
        .await;
        assert!(created["id"].is_i64(), "{created}");
    }
    let created = call(
        &client,
        "connections_create",
        json!({ "name": "c", "host": "h.example" }),
    )
    .await;
    assert!(created["id"].is_i64(), "{created}");

    // In display order: prod, prod/a, prod/b, c.
    let first = call(&client, "connections_list", json!({ "limit": 2 })).await;
    assert_eq!(first["total"], 4, "{first}");
    assert_eq!(first["truncated"], true);
    assert_eq!(first["folders"][0]["name"], "prod");
    assert_eq!(names(&first["folders"][0]["connections"]), ["a"]);
    assert_eq!(names(&first["connections"]), Vec::<String>::new());
    let cursor = first["next_cursor"].as_str().expect("a cursor").to_owned();

    let second = call(
        &client,
        "connections_list",
        json!({ "limit": 2, "cursor": cursor }),
    )
    .await;
    assert_eq!(second["truncated"], false, "{second}");
    assert!(second.get("next_cursor").is_none(), "{second}");
    assert_eq!(second["folders"][0]["name"], "prod", "the folder around b");
    assert_eq!(names(&second["folders"][0]["connections"]), ["b"]);
    assert_eq!(names(&second["connections"]), ["c"]);

    let whole = call(&client, "connections_list", json!({})).await;
    assert_eq!(whole["truncated"], false, "{whole}");
    assert_eq!(names(&whole["folders"][0]["connections"]), ["a", "b"]);
    assert_eq!(names(&whole["connections"]), ["c"]);

    let foreign = call(&client, "connections_list", json!({ "cursor": "0-1-2-3" })).await;
    assert_eq!(foreign["code"], "invalid_params", "{foreign}");
}
