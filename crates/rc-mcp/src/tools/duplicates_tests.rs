//! `files_find_duplicates`, `panel_select_duplicates` and `files_disk_usage` over an in-process
//! pipe: groups page by page, a fed result selected, a folder by size.

use std::{fs, time::Duration};

use rc_engine::{Engine, PanelId};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

/// `root/` with `a.txt` and `sub/a.txt` ("same"), `b.txt` ("diff", the same size), `big.bin`
/// (100 bytes) and `sub/c.txt` ("x").
async fn context() -> (McpContext, TempDir) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("sub")).expect("mkdir");
    fs::write(root.join("a.txt"), "same").expect("write");
    fs::write(root.join("sub/a.txt"), "same").expect("write");
    fs::write(root.join("b.txt"), "diff").expect("write");
    fs::write(root.join("big.bin"), [7_u8; 100]).expect("write");
    fs::write(root.join("sub/c.txt"), "x").expect("write");
    let built = fixture.build().await;
    (built.context, built.root)
}

fn group_names(page: &Value) -> Vec<Vec<String>> {
    page["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .map(|group| {
            group["files"]
                .as_array()
                .expect("files")
                .iter()
                .map(|file| file["name"].as_str().expect("name").to_owned())
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn duplicates_by_size_and_by_contents() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let by_size = call(&client, "files_find_duplicates", json!({})).await;
    assert_eq!(group_names(&by_size), [vec!["a.txt", "b.txt", "a.txt"]]);
    let by_contents = call(
        &client,
        "files_find_duplicates",
        json!({ "same_contents": true }),
    )
    .await;
    assert_eq!(group_names(&by_contents), [vec!["a.txt", "a.txt"]]);
    assert_eq!(by_contents["groups"][0]["size"], 4);
    assert_eq!(by_contents["stats"]["hits"], 2);
    let named = call(
        &client,
        "files_find_duplicates",
        json!({ "same_size": true, "name": "b*" }),
    )
    .await;
    assert_eq!(
        named["groups"],
        json!([]),
        "the masks narrow the files compared"
    );
}

async fn settled(engine: &Engine) {
    for _ in 0..500 {
        let state = engine.panel(PanelId::Left);
        if state.flat.as_ref().is_some_and(|flat| !flat.loading) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the flat view did not finish");
}

#[tokio::test]
async fn a_fed_result_is_selected_leaving_one_per_group() {
    let (context, root) = context().await;
    let client = connect(&context).await;
    let refused = call(
        &client,
        "panel_select_duplicates",
        json!({ "rule": { "by": "mask", "mask": "*" } }),
    )
    .await;
    assert_eq!(refused["code"], "invalid_search");
    let fed = call(
        &client,
        "panel_feed_paths",
        json!({
            "paths": [
                root.path().join("a.txt").display().to_string(),
                root.path().join("sub").join("a.txt").display().to_string(),
            ],
            "groups": [0, 0],
        }),
    )
    .await;
    assert_eq!(fed["flat"]["source"], "duplicates");
    settled(&context.engine).await;
    let state = call(
        &client,
        "panel_select_duplicates",
        json!({ "rule": { "by": "mask", "mask": "*.txt" }, "leave_one": true, "keep": ["first"] }),
    )
    .await;
    let selected = state["selection"].as_array().expect("selection");
    assert_eq!(selected.len(), 1, "{state}");
}

#[tokio::test]
async fn a_folder_by_size() {
    let (context, _root) = context().await;
    let client = connect(&context).await;
    let usage = call(&client, "files_disk_usage", json!({ "limit": 1 })).await;
    assert_eq!(usage["kind"], "disk_usage");
    assert_eq!(usage["usage"]["bytes"], 113);
    assert_eq!(usage["folders"][0]["name"], "sub");
    assert_eq!(usage["largest"][0]["bytes"], 100);
    assert_eq!(usage["largest"].as_array().map(Vec::len), Some(1));
}
