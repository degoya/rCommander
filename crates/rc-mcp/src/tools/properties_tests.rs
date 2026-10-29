//! The properties tools against a real engine on a temporary folder.

use serde_json::json;
use tempfile::TempDir;

use super::{parse_mode, parse_time};
use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

/// `root/` with `dir/inner.txt` (3 bytes), `top.txt` (5 bytes) and `.ssh/id_ed25519`.
async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    let root = fixture.root();
    std::fs::create_dir(root.join("dir")).expect("dir");
    std::fs::write(root.join("dir").join("inner.txt"), b"abc").expect("inner");
    std::fs::write(root.join("top.txt"), b"hello").expect("top");
    std::fs::create_dir(root.join(".ssh")).expect(".ssh");
    std::fs::write(root.join(".ssh").join("id_ed25519"), b"key").expect("key");
    let built = fixture.build().await;
    (built.root, built.context)
}

#[tokio::test]
async fn files_properties_and_files_dir_size_read_metadata() {
    let (_root, context) = setup().await;
    let client = connect(&context).await;
    let stat = call(
        &client,
        "files_properties",
        json!({ "paths": ["dir", "top.txt"], "count": true }),
    )
    .await;
    assert_eq!(stat["kind"], "properties", "{stat}");
    assert_eq!(stat["items"][0]["usage"]["bytes"], 3, "{stat}");
    assert_eq!(stat["items"][1]["entry"]["size"], 5, "{stat}");
    assert_eq!(stat["total"]["bytes"], 8, "{stat}");
    assert_eq!(stat["capabilities"]["links"]["hardlink"], true, "{stat}");

    let sizes = call(&client, "files_dir_size", json!({ "paths": ["dir"] })).await;
    assert_eq!(sizes["kind"], "dir_sizes", "{sizes}");
    assert_eq!(sizes["total"]["files"], 1, "{sizes}");
}

#[tokio::test]
async fn files_set_attributes_takes_rfc3339_and_refuses_keys() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let answer = call(
        &client,
        "files_set_attributes",
        json!({ "paths": ["top.txt"], "modified": "2020-01-02T03:04:05Z", "readonly": true }),
    )
    .await;
    assert_eq!(answer["kind"], "attributes_set", "{answer}");
    assert_eq!(answer["changed"], 1, "{answer}");
    let meta = std::fs::metadata(root.path().join("top.txt")).expect("meta");
    assert!(meta.permissions().readonly());
    let bad = call(
        &client,
        "files_set_attributes",
        json!({ "paths": ["top.txt"], "modified": "yesterday" }),
    )
    .await;
    assert_eq!(bad["code"], "property_invalid", "{bad}");
    let key = call(
        &client,
        "files_set_attributes",
        json!({ "paths": [".ssh/id_ed25519"], "readonly": false }),
    )
    .await;
    assert_eq!(key["code"], "sensitive_path", "{key}");
    let tree = call(
        &client,
        "files_set_attributes",
        json!({ "paths": [".ssh"], "recursive": true, "readonly": false }),
    )
    .await;
    assert_eq!(tree["code"], "sensitive_path", "{tree}");
}

#[tokio::test]
async fn files_create_link_makes_a_hard_link_but_never_to_a_key() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let answer = call(
        &client,
        "files_create_link",
        json!({ "target": "top.txt", "link": "again.txt", "kind": "hardlink" }),
    )
    .await;
    assert_eq!(answer["kind"], "link_created", "{answer}");
    assert_eq!(
        std::fs::read(root.path().join("again.txt")).expect("read"),
        b"hello"
    );
    let taken = call(
        &client,
        "files_create_link",
        json!({ "target": "top.txt", "link": "again.txt", "kind": "hardlink" }),
    )
    .await;
    assert_eq!(taken["code"], "already_exists", "{taken}");
    for (target, link) in [(".ssh/id_ed25519", "copy"), ("top.txt", ".ssh/planted")] {
        let refused = call(
            &client,
            "files_create_link",
            json!({ "target": target, "link": link, "kind": "hardlink" }),
        )
        .await;
        assert_eq!(refused["code"], "sensitive_path", "{refused}");
    }
    assert!(!root.path().join("copy").exists());
}

#[tokio::test]
async fn files_set_comment_writes_descript_ion() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let answer = call(
        &client,
        "files_set_comment",
        json!({ "path": "top.txt", "comment": "greeting" }),
    )
    .await;
    assert_eq!(answer["comment"], "greeting", "{answer}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("descript.ion")).expect("read"),
        "top.txt greeting\r\n"
    );
    let stat = call(&client, "files_properties", json!({ "paths": ["top.txt"] })).await;
    assert_eq!(stat["items"][0]["comment"], "greeting", "{stat}");
}

#[test]
fn times_and_modes_parse() {
    assert!(parse_time("now").is_some());
    assert_eq!(
        parse_time("1970-01-01T00:00:10Z"),
        Some(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10))
    );
    assert_eq!(parse_time("2026-13-01"), None);
    assert_eq!(parse_mode("644"), Some(0o644));
    assert_eq!(parse_mode("0755"), Some(0o755));
    assert_eq!(parse_mode("8"), None);
    assert_eq!(parse_mode("17777"), None);
}
