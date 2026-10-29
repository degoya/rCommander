//! The archive tools end to end: an rmcp client over an in-process pipe.

use std::{fs, path::Path, time::Duration};

use rc_engine::{CommandClass, JobSnapshot, JobState};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};
use crate::tools::class_of as escalate;

/// `root/` with `site/index.html` and `site/css/a.css`; both panels on `root`.
async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir_all(root.join("site").join("css")).expect("mkdir");
    fs::write(root.join("site").join("index.html"), b"<html>").expect("write");
    fs::write(root.join("site").join("css").join("a.css"), b"a{}").expect("write");
    // No window in a test: a confirmation code stands in for the person's answer.
    let built = fixture.with_codes().build().await;
    (built.root, built.context)
}

async fn finished(context: &McpContext, answer: &Value) -> JobSnapshot {
    let id = answer["id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a job: {answer}"));
    let wait = context.engine().ops().wait(id);
    tokio::time::timeout(Duration::from_secs(20), wait)
        .await
        .expect("in time")
        .expect("known")
}

#[test]
fn losing_data_is_destructive() {
    let args = |value: Value| value.as_object().cloned();
    let write = CommandClass::Write;
    let overwrite = args(json!({ "on_conflict": "overwrite_older" }));
    assert_eq!(
        escalate("archives_extract", overwrite.as_ref(), write),
        CommandClass::Destructive
    );
    let skip = args(json!({ "on_conflict": "skip" }));
    assert_eq!(escalate("archives_extract", skip.as_ref(), write), write);
    for flag in ["delete_sources", "overwrite"] {
        let lose = args(json!({ flag: true }));
        assert_eq!(
            escalate("archives_pack", lose.as_ref(), write),
            CommandClass::Destructive,
            "{flag}"
        );
    }
    assert_eq!(escalate("archives_pack", None, write), write);
    assert_eq!(escalate("ops_mkdir", overwrite.as_ref(), write), write);
}

/// Acceptance: archives_pack packs a folder; archives_extract unpacks it again.
#[tokio::test]
async fn pack_then_extract_a_folder() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let target = root.path().join("site.7z");
    let packed = call(
        &client,
        "archives_pack",
        json!({ "sources": ["site"], "target": target, "format": "7z" }),
    )
    .await;
    let done = finished(&context, &packed).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(target.is_file());

    let out = root.path().join("out");
    let unpacked = call(
        &client,
        "archives_extract",
        json!({ "archives": ["site.7z"], "target": out }),
    )
    .await;
    let done = finished(&context, &unpacked).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let css = fs::read(out.join("site").join("css").join("a.css")).expect("unpacked");
    assert_eq!(css, b"a{}");
}

/// Acceptance: with "delete sources" archives_pack wants a confirmation code.
#[tokio::test]
async fn deleting_the_sources_needs_a_confirmation_code() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let asked = call(
        &client,
        "archives_pack",
        json!({ "sources": ["site"], "delete_sources": true }),
    )
    .await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive");
    assert!(root.path().join("site").join("index.html").is_file());
}

/// H2: a folder holding a key anywhere below it is not packed, and the refusal names no file.
#[tokio::test]
async fn a_folder_holding_keys_is_refused_without_naming_them() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let keys = root.path().join("home").join("deep").join(".ssh");
    fs::create_dir_all(&keys).expect("mkdir");
    fs::write(
        keys.join("id_ed25519"),
        b"-----BEGIN OPENSSH PRIVATE KEY-----",
    )
    .expect("write");
    let refused = call(&client, "archives_pack", json!({ "sources": ["home"] })).await;
    assert_eq!(refused["code"], "sensitive_path", "{refused}");
    let text = refused.to_string();
    assert!(
        !text.contains("id_ed25519") && !text.contains(".ssh"),
        "{text}"
    );
    assert!(!root.path().join("home.zip").exists());
}

/// Acceptance: an encrypted archive reports password_required; no tool takes a password.
#[tokio::test]
async fn an_encrypted_archive_reports_password_required() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives/encrypted.7z");
    let answer = call(
        &client,
        "archives_extract",
        json!({ "archives": [fixture], "target": root.path().join("x") }),
    )
    .await;
    let done = finished(&context, &answer).await;
    let codes: Vec<&str> = done.failures.iter().map(|f| f.code.as_str()).collect();
    assert_eq!(codes, ["password_required"]);
}
