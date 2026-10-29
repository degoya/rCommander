//! The `multi_rename` tools end to end: an rmcp client over an in-process pipe.

use std::{fs, time::Duration};

use rc_engine::JobState;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};

/// `root/` with `a.txt`, `b.txt` and `c.txt`, each holding its own name.
async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(fixture.root().join(name), name).expect("write");
    }
    let built = fixture.build().await;
    (built.root, built.context)
}

/// Waits for the job of a rename run; its state.
async fn finished(context: &McpContext, answer: &Value) -> JobState {
    let id = answer["job"]["id"]
        .as_u64()
        .unwrap_or_else(|| panic!("a rename run: {answer}"));
    tokio::time::timeout(Duration::from_secs(10), context.engine().ops().wait(id))
        .await
        .expect("in time")
        .expect("known")
        .state
}

fn content(root: &TempDir, name: &str) -> String {
    fs::read_to_string(root.path().join(name)).expect("read")
}

#[tokio::test]
async fn preview_apply_and_undo_a_swap() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let args = json!({
        "names": ["a.txt", "b.txt"],
        "search": { "find": "a|b", "replace": "b|a", "case_sensitive": true }
    });

    let preview = call(&client, "multi_rename_preview", args.clone()).await;
    assert_eq!(preview["kind"], "rename_preview", "{preview}");
    assert_eq!(preview["plan"]["items"][0]["new"], "b.txt");
    assert_eq!(preview["plan"]["items"][0]["change"], "rename");
    assert!(preview["plan"]["items"][0]["conflict"].is_null());
    assert_eq!(
        content(&root, "a.txt"),
        "a.txt",
        "a preview renames nothing"
    );

    let applied = call(&client, "multi_rename_apply", args).await;
    assert_eq!(applied["kind"], "rename_run", "{applied}");
    assert_eq!(finished(&context, &applied).await, JobState::Completed);
    assert_eq!(content(&root, "a.txt"), "b.txt");
    assert_eq!(content(&root, "b.txt"), "a.txt");

    let undone = call(&client, "multi_rename_undo", json!({})).await;
    assert_eq!(finished(&context, &undone).await, JobState::Completed);
    assert_eq!(content(&root, "a.txt"), "a.txt");
    assert_eq!(content(&root, "b.txt"), "b.txt");
}

#[tokio::test]
async fn apply_renames_nothing_when_the_preview_has_conflicts() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let refused = call(
        &client,
        "multi_rename_apply",
        json!({ "names": ["a.txt", "b.txt"], "name_mask": "same" }),
    )
    .await;
    assert_eq!(refused["code"], "rename_plan_conflicts", "{refused}");
    assert_eq!(content(&root, "a.txt"), "a.txt");
    assert_eq!(content(&root, "b.txt"), "b.txt");

    let bad_mask = call(
        &client,
        "multi_rename_preview",
        json!({ "names": ["a.txt"], "name_mask": "[N" }),
    )
    .await;
    assert_eq!(bad_mask["code"], "rename_mask_unclosed", "{bad_mask}");

    let nothing = call(&client, "multi_rename_undo", json!({})).await;
    assert_eq!(nothing["code"], "rename_nothing_to_undo", "{nothing}");
}

#[tokio::test]
async fn the_counter_and_case_reach_the_rule() {
    let (_root, context) = setup().await;
    let client = connect(&context).await;
    let preview = call(
        &client,
        "multi_rename_preview",
        json!({
            "names": ["c.txt", "a.txt"],
            "name_mask": "photo_[C]",
            "case": "upper",
            "counter_start": 9,
            "counter_width": 3
        }),
    )
    .await;
    let news: Vec<&str> = preview["plan"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("items: {preview}"))
        .iter()
        .filter_map(|item| item["new"].as_str())
        .collect();
    assert_eq!(news, ["PHOTO_009.TXT", "PHOTO_010.TXT"]);
}

#[tokio::test]
async fn a_key_is_never_renamed_out_of_the_guard() {
    let (root, context) = setup().await;
    fs::create_dir(root.path().join(".ssh")).expect("mkdir");
    fs::write(root.path().join(".ssh").join("id_ed25519"), "KEY").expect("write");
    let client = connect(&context).await;

    let refused = call(
        &client,
        "multi_rename_apply",
        json!({ "dir": ".ssh", "names": ["id_ed25519"], "name_mask": "x", "extension_mask": "txt" }),
    )
    .await;
    assert_eq!(refused["code"], "sensitive_path", "{refused}");
    assert!(root.path().join(".ssh").join("id_ed25519").exists());
    assert!(!root.path().join(".ssh").join("x.txt").exists());
}

#[tokio::test]
async fn no_file_is_renamed_to_look_like_a_key() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let refused = call(
        &client,
        "multi_rename_apply",
        json!({ "names": ["a.txt"], "name_mask": "server", "extension_mask": "pem" }),
    )
    .await;
    assert_eq!(refused["code"], "sensitive_path", "{refused}");
    assert_eq!(content(&root, "a.txt"), "a.txt");
    assert!(!root.path().join("server.pem").exists());
}

#[tokio::test]
async fn an_undo_that_touches_a_sensitive_name_is_refused() {
    let (root, context) = setup().await;
    // The person renamed a.txt to server.key in the window: not an MCP decision to reverse.
    let request = rc_engine::MultiRenameRequest {
        names: vec!["a.txt".into()],
        rule: rc_engine::RenameRule {
            name_mask: "server".into(),
            extension_mask: "key".into(),
            ..rc_engine::RenameRule::default()
        },
        ..rc_engine::MultiRenameRequest::default()
    };
    let rc_engine::Outcome::RenameRun { job, .. } = context
        .engine()
        .dispatch(rc_engine::Command::MultiRenameApply(request))
        .await
        .expect("queued")
    else {
        panic!("a rename run");
    };
    tokio::time::timeout(Duration::from_secs(10), context.engine().ops().wait(job.id))
        .await
        .expect("in time")
        .expect("known");
    assert!(root.path().join("server.key").exists());

    let client = connect(&context).await;
    let refused = call(&client, "multi_rename_undo", json!({})).await;
    assert_eq!(refused["code"], "sensitive_path", "{refused}");
    assert!(
        root.path().join("server.key").exists(),
        "nothing was undone"
    );
}

#[tokio::test]
async fn a_preset_is_saved_listed_renamed_and_deleted() {
    let (_root, context) = setup().await;
    let client = connect(&context).await;
    let saved = call(
        &client,
        "multi_rename_preset_save",
        json!({
            "name": "Photos",
            "name_mask": "[Y]-[M]-[D]_[C]",
            "case": "lower",
            "counter_width": 3
        }),
    )
    .await;
    assert_eq!(saved["kind"], "rename_presets", "{saved}");
    let listed = call(&client, "multi_rename_preset_list", json!({})).await;
    let preset = &listed["presets"][0];
    assert_eq!(preset["name"], "Photos", "{listed}");
    assert_eq!(preset["rule"]["name_mask"], "[Y]-[M]-[D]_[C]");
    assert_eq!(preset["rule"]["extension_mask"], "[E]");
    assert_eq!(preset["rule"]["case"], "lower");
    assert_eq!(preset["rule"]["counter"]["width"], 3);

    let renamed = call(
        &client,
        "multi_rename_preset_rename",
        json!({ "name": "photos", "new_name": "Pictures" }),
    )
    .await;
    assert_eq!(renamed["presets"][0]["name"], "Pictures", "{renamed}");
    // Destructive as hotlist_delete is (RC-080-12): headless, a code answers the question.
    crate::ask::allow_headless_codes(&context.db).await;
    let asked = call(
        &client,
        "multi_rename_preset_delete",
        json!({ "name": "Pictures" }),
    )
    .await;
    assert_eq!(asked["class"], "destructive", "{asked}");
    let deleted = call(
        &client,
        "multi_rename_preset_delete",
        json!({ "name": "Pictures", "confirmation": asked["confirmation"] }),
    )
    .await;
    assert_eq!(deleted["presets"], json!([]), "{deleted}");
}
