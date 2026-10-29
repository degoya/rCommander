//! `files_search` end to end over an in-process pipe: the same hits as the engine's streaming
//! search (what the find-files dialog shows), paging, and refusals with their codes.

use std::fs;

use rc_engine::{SearchEvent, SearchQuery, search::ContentQuery};
use rmcp::{RoleClient, model::CallToolRequestParams, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, connect};

/// `root/` with `app.log` ("an error"), `ok.log` ("fine"), `notes.txt` and `sub/old.log`
/// ("error").
async fn context() -> (McpContext, TempDir) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("sub")).expect("mkdir");
    fs::write(root.join("app.log"), "an error").expect("write");
    fs::write(root.join("ok.log"), "fine").expect("write");
    fs::write(root.join("notes.txt"), "error").expect("write");
    fs::write(root.join("sub/old.log"), "error").expect("write");
    let built = fixture.build().await;
    (built.context, built.root)
}

async fn call(client: &RunningService<RoleClient, ()>, arguments: Value) -> Value {
    let arguments = arguments.as_object().cloned().unwrap_or_default();
    let result = client
        .call_tool(CallToolRequestParams::new("files_search".to_owned()).with_arguments(arguments))
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

fn paths(page: &Value) -> Vec<String> {
    page["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .map(|hit| hit["path"].as_str().expect("path").to_owned())
        .collect()
}

#[tokio::test]
async fn the_tool_finds_what_the_dialog_finds() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let page = call(
        &client,
        json!({ "name": "*.log", "content": { "text": "ERROR" } }),
    )
    .await;
    assert!(page.get("next_cursor").is_none(), "{page}");
    assert_eq!(page["stats"]["hits"], 2);
    assert_eq!(page["hits"][0]["name"], "app.log");
    assert_eq!(page["hits"][0]["kind"], "file");

    let query = SearchQuery {
        name: "*.log".to_owned(),
        content: Some(ContentQuery {
            text: "ERROR".to_owned(),
            ..ContentQuery::default()
        }),
        ..SearchQuery::default()
    };
    let mut handle = context
        .engine()
        .start_search(None, &[], &query)
        .await
        .expect("start");
    let mut streamed = Vec::new();
    while let Some(event) = handle.next().await {
        if let SearchEvent::Hit(hit) = event {
            streamed.push(hit.path.path.to_string_lossy().into_owned());
        }
    }
    assert_eq!(paths(&page), streamed);
}

#[tokio::test]
async fn pages_follow_the_cursor() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let first = call(&client, json!({ "name": "*", "limit": 2 })).await;
    let cursor = first["next_cursor"].as_str().expect("more").to_owned();
    let second = call(
        &client,
        json!({ "name": "*", "limit": 2, "cursor": cursor }),
    )
    .await;
    let whole = call(&client, json!({ "name": "*" })).await;
    let mut paged = paths(&first);
    paged.extend(paths(&second));
    assert_eq!(paged, paths(&whole)[..4]);
}

#[tokio::test]
async fn refusals_carry_their_codes() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let broken = call(&client, json!({ "name": "(", "name_regex": true })).await;
    assert_eq!(broken["code"], "invalid_search");
    let empty = call(&client, json!({ "content": { "text": "" } })).await;
    assert_eq!(empty["code"], "invalid_search");
    let foreign = call(&client, json!({ "cursor": "4-0000000000000000" })).await;
    assert_eq!(foreign["code"], "invalid_search");
    let missing = call(&client, json!({ "paths": ["gone"] })).await;
    assert_eq!(missing["code"], "not_found");
}

/// A content hit tells what a file holds: the engine never reads keys and credential files,
/// so they are no hits whatever they hold - also not as files `not_containing` a text - and only
/// count as skipped, while a search by name still lists them like the panel does.
#[tokio::test]
async fn a_content_search_never_reads_sensitive_files() {
    let (context, root) = context().await;
    fs::create_dir(root.path().join(".ssh")).expect("mkdir");
    fs::write(root.path().join(".ssh/id_ed25519"), "an error").expect("write");
    fs::write(root.path().join("server.pem"), "an error").expect("write");
    let client = connect(&context).await;
    let page = call(&client, json!({ "content": { "text": "error" } })).await;
    let found = paths(&page);
    assert!(
        found
            .iter()
            .all(|path| !path.contains(".ssh") && !path.ends_with(".pem")),
        "{page}"
    );
    assert!(found.iter().any(|path| path.ends_with("app.log")), "{page}");
    assert_eq!(page["stats"]["hits"], found.len(), "{page}");
    assert_eq!(page["stats"]["skipped"], 2, "{page}");
    assert!(page.get("withheld").is_none(), "{page}");
    let absent = call(
        &client,
        json!({ "content": { "text": "no such text", "not_containing": true } }),
    )
    .await;
    assert!(
        paths(&absent)
            .iter()
            .all(|path| !path.contains(".ssh") && !path.ends_with(".pem")),
        "{absent}"
    );
    assert_eq!(absent["stats"]["skipped"], 2, "{absent}");
    let by_name = call(&client, json!({ "name": "*.pem" })).await;
    assert_eq!(paths(&by_name).len(), 1, "{by_name}");
}

/// A content search does not start in a folder the guard keeps from MCP.
#[tokio::test]
async fn a_content_search_refuses_a_sensitive_start_folder() {
    let (context, root) = context().await;
    fs::create_dir(root.path().join(".ssh")).expect("mkdir");
    let client = connect(&context).await;
    let refused = call(
        &client,
        json!({ "paths": [".ssh"], "content": { "text": "x" } }),
    )
    .await;
    assert_eq!(refused["code"], "sensitive_path", "{refused}");
    let by_name = call(&client, json!({ "paths": [".ssh"], "name": "*" })).await;
    assert!(by_name.get("code").is_none(), "{by_name}");
}

/// A key reached through a followed link to its folder is judged where it really is.
#[cfg(unix)]
#[tokio::test]
async fn a_followed_link_does_not_hide_a_key() {
    let (context, root) = context().await;
    let keys = tempfile::tempdir().expect("keys");
    fs::create_dir(keys.path().join(".ssh")).expect("mkdir");
    fs::write(keys.path().join(".ssh/id_rsa"), "an error").expect("write");
    std::os::unix::fs::symlink(keys.path().join(".ssh"), root.path().join("plain")).expect("link");
    let client = connect(&context).await;
    let page = call(
        &client,
        json!({ "content": { "text": "error" }, "follow_symlinks": true }),
    )
    .await;
    assert!(
        paths(&page).iter().all(|path| !path.contains("plain")),
        "{page}"
    );
    assert_eq!(page["stats"]["skipped"], 1, "{page}");
}
