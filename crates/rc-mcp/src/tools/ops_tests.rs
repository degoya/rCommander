//! The `ops` tools end to end: an rmcp client over an in-process pipe.

use std::{fs, time::Duration};

use rc_engine::{CommandClass, JobState};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};
use crate::tools::class_of as escalate;

/// `root/` with `a.txt` and `docs/a.txt` (an older copy); both panels on `root`.
async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir(root.join("docs")).expect("mkdir");
    fs::write(root.join("a.txt"), b"new").expect("write");
    fs::write(root.join("docs").join("a.txt"), b"old").expect("write");
    // Headless: these tests drive the confirmation-code path (RC-010-13).
    let built = fixture.with_codes().build().await;
    (built.root, built.context)
}

/// Waits for the job an answer names; its state.
async fn finished(context: &McpContext, answer: &Value) -> JobState {
    let id = answer["id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a job: {answer}"));
    let wait = context.engine().ops().wait(id);
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("in time")
        .expect("known")
        .state
}

#[tokio::test]
async fn ops_delete_without_a_valid_code_is_refused() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let target = root.path().join("a.txt");
    let args = json!({ "sources": ["a.txt"], "permanent": true });

    let asked = call(&client, "ops_delete", args.clone()).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive");
    let code = asked["confirmation"].as_str().expect("code").to_owned();

    let mut forged = args.clone();
    forged["confirmation"] = json!("0123456789abcdef0123456789abcdef");
    let refused = call(&client, "ops_delete", forged).await;
    assert_eq!(refused["confirmation_required"], true, "{refused}");
    assert!(target.exists(), "nothing is deleted without a valid code");
    // The forged try spent nothing but itself: the real code was a different one.
    let mut confirmed = args.clone();
    confirmed["confirmation"] = json!(code);
    let queued = call(&client, "ops_delete", confirmed).await;
    assert_eq!(
        finished(&context, &queued).await,
        JobState::Completed,
        "{queued}"
    );
    assert!(!target.exists());
}

#[tokio::test]
async fn ops_copy_skips_conflicts_without_asking() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let queued = call(
        &client,
        "ops_copy",
        json!({ "sources": ["a.txt"], "target": "docs" }),
    )
    .await;
    assert_eq!(
        finished(&context, &queued).await,
        JobState::Completed,
        "{queued}"
    );
    assert_eq!(
        fs::read(root.path().join("docs").join("a.txt")).expect("read"),
        b"old"
    );

    let listed = call(&client, "ops_list", json!({})).await;
    let job = &listed["jobs"][0];
    assert_eq!(job["state"], "completed", "{listed}");
    assert_eq!(job["skipped"], 1);
    assert_eq!(job["progress"]["files_done"], 1);
    // The file at work's own bytes (RC-010-07): none once the job is over.
    assert_eq!(job["progress"]["file_bytes_done"], 0, "{listed}");
    assert_eq!(job["progress"]["file_bytes_total"], 0, "{listed}");
}

#[tokio::test]
async fn overwriting_needs_a_confirmation_code() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let args = json!({ "sources": ["a.txt"], "target": "docs", "on_conflict": "overwrite" });
    let asked = call(&client, "ops_copy", args.clone()).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive");
    let mut confirmed = args;
    confirmed["confirmation"] = asked["confirmation"].clone();
    let queued = call(&client, "ops_copy", confirmed).await;
    assert_eq!(
        finished(&context, &queued).await,
        JobState::Completed,
        "{queued}"
    );
    assert_eq!(
        fs::read(root.path().join("docs").join("a.txt")).expect("read"),
        b"new"
    );
}

#[tokio::test]
async fn steering_a_finished_or_unknown_job_is_an_error() {
    let (_root, context) = setup().await;
    let client = connect(&context).await;
    let queued = call(&client, "ops_mkdir", json!({ "path": "made" })).await;
    finished(&context, &queued).await;
    let id = queued["id"].clone();
    let late = call(&client, "ops_pause", json!({ "job": id })).await;
    assert_eq!(late["code"], "job_finished", "{late}");
    let unknown = call(&client, "ops_cancel", json!({ "job": 999 })).await;
    assert_eq!(unknown["code"], "job_not_found", "{unknown}");
}

#[test]
fn only_an_overwriting_transfer_is_escalated() {
    let args = |value: Value| value.as_object().cloned().expect("object");
    let overwrite = args(json!({ "on_conflict": "overwrite" }));
    let older = args(json!({ "on_conflict": "overwrite_older" }));
    let skip = args(json!({ "on_conflict": "skip" }));
    let larger = args(json!({ "on_conflict": "overwrite_larger" }));
    let aside = args(json!({ "on_conflict": "rename_existing" }));
    let write = CommandClass::Write;
    assert_eq!(
        escalate("ops_copy", Some(&overwrite), write),
        CommandClass::Destructive
    );
    assert_eq!(
        escalate("ops_move", Some(&older), write),
        CommandClass::Destructive
    );
    assert_eq!(escalate("ops_copy", Some(&skip), write), write);
    assert_eq!(
        escalate("ops_copy", Some(&larger), write),
        CommandClass::Destructive
    );
    assert_eq!(escalate("ops_move", Some(&aside), write), write);
    // Resuming continues the copy's own part file: nothing of the target's is lost (RC-060-07).
    let resume = args(json!({ "on_conflict": "resume" }));
    assert_eq!(escalate("ops_copy", Some(&resume), write), write);
    assert_eq!(escalate("ops_copy", None, write), write);
    assert_eq!(escalate("ops_mkdir", Some(&overwrite), write), write);
}

/// A copy or a move would carry a key's contents to a name the guard no longer knows (outside
/// `.aws`, or `credentials (2)` after a conflict): both refuse sources that are or hold a
/// sensitive file, and queue no job.
#[tokio::test]
async fn ops_copy_and_move_refuse_sensitive_sources() {
    let (root, context) = setup().await;
    fs::create_dir(root.path().join(".aws")).expect("mkdir");
    fs::write(root.path().join(".aws").join("credentials"), b"key").expect("write");
    let client = connect(&context).await;
    for (tool, source) in [
        ("ops_copy", ".aws/credentials"),
        ("ops_copy", ".aws"),
        ("ops_move", ".aws"),
    ] {
        let refused = call(
            &client,
            tool,
            json!({ "sources": [source], "target": "docs" }),
        )
        .await;
        assert_eq!(
            refused["code"], "sensitive_path",
            "{tool} {source}: {refused}"
        );
    }
    assert!(context.engine().ops().jobs().is_empty());
    assert!(!root.path().join("docs").join(".aws").exists());
}

/// `ops_mkdir` judges the place on the server a `sftp://` location names, not that text joined
/// onto the panel's folder (RC-060-05): an SSH folder there is refused, an unknown connection
/// answers its code - both before any job is queued.
#[tokio::test]
async fn ops_mkdir_judges_a_server_s_location_where_it_creates() {
    let (root, context) = setup().await;
    context
        .engine()
        .dispatch(rc_engine::Command::CreateConnection {
            folder_id: None,
            connection: rc_engine::ConnectionDraft {
                username: Some("tester".to_owned()),
                ..rc_engine::ConnectionDraft::new("box", "127.0.0.1")
            },
        })
        .await
        .expect("connection");
    let client = connect(&context).await;
    let refused = call(
        &client,
        "ops_mkdir",
        json!({ "path": "sftp://box/home/tester/.ssh/keys" }),
    )
    .await;
    assert_eq!(refused["code"], "protected_target", "{refused}");
    let unknown = call(&client, "ops_mkdir", json!({ "path": "sftp://nobody/x" })).await;
    assert_eq!(unknown["code"], "sftp_connection_not_found", "{unknown}");
    assert!(context.engine().ops().jobs().is_empty());
    assert!(!root.path().join("sftp:").exists());
}
