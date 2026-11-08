//! The archive change tools end to end (RC-110-05): an rmcp client over an in-process pipe.

use std::{fs, path::Path, time::Duration};

use rc_engine::{Command, CommandClass, JobSnapshot, JobState, Outcome, PanelId};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::McpContext;
use crate::test_support::{Fixture, call, connect};
use crate::tools::class_of as escalate;

/// `root/` with `site/index.html` and `extra.txt`; both panels on `root`.
async fn setup() -> (TempDir, McpContext) {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::create_dir_all(root.join("site")).expect("mkdir");
    fs::write(root.join("site").join("index.html"), b"<html>").expect("write");
    fs::write(root.join("extra.txt"), b"extra").expect("write");
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

/// Whether the archive `zip` holds `name` in its folder `dir`, as `archive.list` lists it.
async fn holds(context: &McpContext, zip: &Path, dir: &str, name: &str) -> bool {
    let listed = context
        .engine()
        .dispatch(Command::ArchiveList {
            // The right panel stays on the folder; the left one goes into the archive.
            panel: Some(PanelId::Right),
            path: Some(zip.to_string_lossy().into_owned()),
            dir: Some(dir.to_owned()),
        })
        .await;
    match listed {
        Ok(Outcome::ArchiveListing { entries, .. }) => {
            entries.iter().any(|item| item.entry.name == name)
        }
        _ => false,
    }
}

#[test]
fn deleting_and_losing_data_are_destructive() {
    let args = |value: Value| value.as_object().cloned();
    let write = CommandClass::Write;
    assert_eq!(escalate("archives_add", None, write), write);
    for lose in [
        json!({ "delete_sources": true }),
        json!({ "on_conflict": "overwrite" }),
    ] {
        assert_eq!(
            escalate("archives_add", args(lose.clone()).as_ref(), write),
            CommandClass::Destructive,
            "{lose}"
        );
    }
}

/// Acceptance: archives_add puts a file into a folder inside a zip; archives_delete, with a
/// confirmation code, takes the folder out again.
#[tokio::test]
async fn add_then_delete_inside_a_zip() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let zip = root.path().join("site.zip");
    let packed = call(
        &client,
        "archives_pack",
        json!({ "sources": ["site"], "target": zip }),
    )
    .await;
    assert_eq!(finished(&context, &packed).await.state, JobState::Completed);

    let added = call(
        &client,
        "archives_add",
        json!({ "sources": ["extra.txt"], "target": zip.join("more") }),
    )
    .await;
    let done = finished(&context, &added).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(holds(&context, &zip, "/more", "extra.txt").await);
    assert!(root.path().join("extra.txt").is_file(), "the source stays");

    context
        .engine()
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: zip.to_string_lossy().into_owned(),
        })
        .await
        .expect("into the archive");
    let args = json!({ "entries": ["more"], "panel": "left" });
    let asked = call(&client, "archives_delete", args.clone()).await;
    assert_eq!(asked["confirmation_required"], true, "{asked}");
    assert_eq!(asked["class"], "destructive");
    assert!(
        holds(&context, &zip, "/more", "extra.txt").await,
        "nothing before the code"
    );
    let mut confirmed = args;
    confirmed["confirmation"] = asked["confirmation"].clone();
    let deleted = call(&client, "archives_delete", confirmed).await;
    let done = finished(&context, &deleted).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(!holds(&context, &zip, "/more", "extra.txt").await);
    assert!(holds(&context, &zip, "/site", "index.html").await);
}

/// A compressed tar is not changed, as Total Commander refuses it.
#[tokio::test]
async fn a_tar_gz_is_not_modifiable() {
    let (root, context) = setup().await;
    let client = connect(&context).await;
    let tgz = root.path().join("tree.tar.gz");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../rc-vfs/tests/fixtures/archives/tree.tar.gz"),
        &tgz,
    )
    .expect("copy");
    let refused = call(
        &client,
        "archives_add",
        json!({ "sources": ["extra.txt"], "target": tgz }),
    )
    .await;
    assert_eq!(refused["code"], "archive_not_modifiable", "{refused}");
}
