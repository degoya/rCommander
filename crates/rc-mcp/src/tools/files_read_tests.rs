//! The MCP boundary of the viewer: capped windows, the sensitive-path guard, masked keys.

use std::fs;

use rmcp::{RoleClient, service::RunningService};
use serde_json::json;
use tempfile::TempDir;

use crate::test_support::{Fixture, call};

async fn setup() -> (TempDir, RunningService<RoleClient, ()>) {
    let built = Fixture::new().build().await;
    let client = built.client().await;
    (built.root, client)
}

fn key_text() -> String {
    format!(
        "-----BEGIN OPENSSH {kind}-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmU\n-----END OPENSSH {kind}-----\n",
        kind = "PRIVATE KEY"
    )
}

#[tokio::test]
async fn a_text_window_says_where_it_ends_and_whether_it_was_cut() {
    let (root, client) = setup().await;
    fs::write(root.path().join("app.log"), "line\n".repeat(1000)).expect("write");
    let first = call(
        &client,
        "files_read_text",
        json!({ "path": "app.log", "max_bytes": 12 }),
    )
    .await;
    assert_eq!(first["kind"], "file_text");
    assert_eq!(first["encoding"], "utf8");
    assert_eq!(first["text"], "line\nline\nli");
    assert_eq!(first["end"], 12);
    assert_eq!(first["truncated"], true);
    assert_eq!(first["size"], 5000);
    let last = call(
        &client,
        "files_read_text",
        json!({ "path": "app.log", "offset": 4995 }),
    )
    .await;
    assert_eq!(last["text"], "line\n");
    assert_eq!(last["truncated"], false);
}

#[tokio::test]
async fn hex_rows_come_as_xxd_prints_them() {
    let (root, client) = setup().await;
    fs::write(root.path().join("hello.bin"), b"Hello, World!\n").expect("write");
    let answer = call(&client, "files_read_hex", json!({ "path": "hello.bin" })).await;
    assert_eq!(
        answer["rows"],
        json!(["00000000: 4865 6c6c 6f2c 2057 6f72 6c64 210a       Hello, World!."])
    );
    assert_eq!(answer["truncated"], false);
}

#[tokio::test]
async fn key_and_credential_files_are_refused() {
    let (root, client) = setup().await;
    let ssh = root.path().join(".ssh");
    fs::create_dir(&ssh).expect("mkdir");
    fs::write(ssh.join("id_ed25519"), key_text()).expect("write");
    fs::write(root.path().join(".env"), "API_TOKEN=x\n").expect("write");
    for (tool, path) in [
        ("files_read_text", ".ssh/id_ed25519"),
        ("files_read_hex", ".ssh/id_ed25519"),
        ("files_read_text", ".env"),
        ("files_read_text", "mcp-instance.json"),
        ("files_read_hex", "rcommander.db-wal"),
        ("files_read_text", "keys/server.KEY"),
    ] {
        let answer = call(&client, tool, json!({ "path": path })).await;
        assert_eq!(answer["code"], "sensitive_path", "{tool} {path}: {answer}");
    }
}

/// A harmless name that links to a key is judged by where it leads.
#[cfg(unix)]
#[tokio::test]
async fn a_link_to_a_key_is_refused_too() {
    let (root, client) = setup().await;
    let ssh = root.path().join(".ssh");
    fs::create_dir(&ssh).expect("mkdir");
    fs::write(ssh.join("id_rsa"), key_text()).expect("write");
    std::os::unix::fs::symlink(ssh.join("id_rsa"), root.path().join("notes.txt")).expect("link");
    let answer = call(&client, "files_read_text", json!({ "path": "notes.txt" })).await;
    assert_eq!(answer["code"], "sensitive_path", "{answer}");
}

#[tokio::test]
async fn key_material_under_another_name_is_masked() {
    let (root, client) = setup().await;
    fs::write(root.path().join("backup.txt"), key_text()).expect("write");
    let text = call(&client, "files_read_text", json!({ "path": "backup.txt" })).await;
    assert_eq!(text["masked"], true);
    assert_eq!(text["text"], "[private key redacted]");
    let hex = call(&client, "files_read_hex", json!({ "path": "backup.txt" })).await;
    assert_eq!(hex["masked"], true);
    assert_eq!(hex["rows"], json!([]));
    let rendered = format!("{text}{hex}");
    assert!(!rendered.contains("b3BlbnNzaC1rZXkt"), "{rendered}");
}

/// rCommander's database files are refused by name - the database, a backup under its generated
/// name - and a copy under any other name by its content.
#[tokio::test]
async fn database_files_are_refused_by_name_and_by_content() {
    let (root, client) = setup().await;
    let copy = root.path().join("notes.bin");
    rc_db::Db::open(copy.clone())
        .await
        .expect("a database")
        .close()
        .await;
    fs::write(root.path().join("rcommander-20261017-120000.db"), b"x").expect("write");
    fs::write(root.path().join("plain.bin"), b"SQLite format 3").expect("write");
    for path in [
        "notes.bin",
        "rcommander-20261017-120000.db",
        "rcommander.db",
    ] {
        let answer = call(&client, "files_read_hex", json!({ "path": path })).await;
        assert_eq!(answer["code"], "sensitive_path", "{path}: {answer}");
    }
    let plain = call(&client, "files_read_hex", json!({ "path": "plain.bin" })).await;
    assert!(plain.get("code").is_none(), "{plain}");
}
