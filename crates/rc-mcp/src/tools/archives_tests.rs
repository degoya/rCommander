//! The MCP boundary of archives: the listing matches the panel, unsafe names are marked, and an
//! archive in a sensitive place is never read.

use std::{fs, path::Path};

use rmcp::{RoleClient, service::RunningService};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::test_support::{Fixture, call};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives")
        .join(name)
}

async fn setup() -> (TempDir, RunningService<RoleClient, ()>) {
    let setup = Fixture::new();
    for name in ["tree.zip", "unsafe.zip"] {
        fs::copy(fixture(name), setup.root().join(name)).expect("copy the fixture");
    }
    let built = setup.build().await;
    let client = built.client().await;
    (built.root, client)
}

fn names(entries: &Value) -> Vec<String> {
    entries
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["name"].as_str().expect("a name").to_owned())
        .collect()
}

#[tokio::test]
async fn archives_list_answers_the_entries_the_panel_shows() {
    let (root, client) = setup().await;
    let listed = call(&client, "archives_list", json!({ "path": "tree.zip" })).await;
    assert_eq!(
        listed["shown"].as_str().map(Path::new),
        Some(root.path().join("tree.zip").as_path())
    );
    let typed = root.path().join("tree.zip");
    let opened = call(
        &client,
        "navigate",
        json!({ "path": typed.to_string_lossy() }),
    )
    .await;
    assert!(opened.get("code").is_none(), "{opened}");
    let panel = call(&client, "list_dir", json!({})).await;
    assert_eq!(names(&listed["entries"]), names(&panel["entries"]));
    let again = call(&client, "archives_list", json!({})).await;
    assert_eq!(names(&again["entries"]), names(&panel["entries"]));
}

#[tokio::test]
async fn archives_list_marks_a_name_that_climbed_out() {
    let (_root, client) = setup().await;
    let listed = call(&client, "archives_list", json!({ "path": "unsafe.zip" })).await;
    let entries = listed["entries"].as_array().expect("entries");
    let evil = entries
        .iter()
        .find(|entry| entry["name"] == "evil.txt")
        .expect("listed under its cleaned name");
    assert_eq!(evil["unsafe_name"], "parent_dir", "{evil}");
    assert_eq!(evil["stored_name"], "../evil.txt", "{evil}");
    let refused = call(&client, "archives_list", json!({ "path": "missing.txt" })).await;
    assert!(refused["code"].is_string(), "{refused}");
}

#[tokio::test]
async fn an_archive_in_a_sensitive_place_is_neither_tested_nor_read() {
    let (root, client) = setup().await;
    let ssh = root.path().join(".ssh");
    fs::create_dir(&ssh).expect("mkdir");
    fs::copy(fixture("tree.zip"), ssh.join("keys.zip")).expect("copy");
    let refused = call(
        &client,
        "archives_test",
        json!({ "paths": [".ssh/keys.zip"] }),
    )
    .await;
    assert_eq!(refused["code"], "sensitive_path", "{refused}");
    let typed = ssh.join("keys.zip");
    call(
        &client,
        "navigate",
        json!({ "path": typed.to_string_lossy() }),
    )
    .await;
    let read = call(&client, "files_read_text", json!({ "path": "hello.txt" })).await;
    assert_eq!(read["code"], "sensitive_path", "{read}");
}

#[tokio::test]
async fn archives_test_queues_a_job_per_archive() {
    let (_root, client) = setup().await;
    let queued = call(&client, "archives_test", json!({ "paths": ["tree.zip"] })).await;
    assert_eq!(queued["jobs"].as_array().map(Vec::len), Some(1), "{queued}");
    let none = call(&client, "archives_test", json!({ "paths": [] })).await;
    assert_eq!(none["code"], "no_sources", "{none}");
}
