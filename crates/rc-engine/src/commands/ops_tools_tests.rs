//! Packing, unpacking, checksums, synchronizing and multi-rename run at once, outside the
//! queue's lists, never in a lane behind a copy - unless `queue.tools_at_once` is off
//! (RC-010-28).

use std::{fs, path::Path, time::Duration};

use rc_ops::{JobId, JobOptions, JobSnapshot, JobState, PackSettings};
use rc_rename::{RenameRule, SearchReplace};
use serde_json::json;

use crate::test_support::{self, expect_job, expect_jobs};
use crate::{
    AT_ONCE, Algorithm, ChecksumFilesRequest, ChecksumJob, Command, DEFAULT_LIST, Engine,
    MultiRenameRequest, Outcome, QueueListId, settings::TOOLS_AT_ONCE_KEY, sync::SyncApplyRequest,
};

/// A copy of `a.txt` into the default list that asks about `hold/a.txt` and keeps its lane,
/// the local one, meanwhile.
async fn holder(engine: &Engine, root: &Path) -> JobId {
    let copy = expect_job(
        engine
            .dispatch(Command::OpsCopyTo {
                panel: None,
                sources: vec![root.join("a.txt").display().to_string()],
                target: root.join("hold").display().to_string(),
                options: JobOptions::default(),
                queue: Some(DEFAULT_LIST),
            })
            .await
            .expect("queued"),
    );
    copy.id
}

async fn finished(engine: &Engine, id: JobId) -> JobSnapshot {
    let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(id))
        .await
        .expect("in time")
        .expect("known");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    done
}

fn list_of(engine: &Engine, id: JobId) -> QueueListId {
    engine
        .ops()
        .entries()
        .into_iter()
        .find(|entry| entry.job.id == id)
        .expect("listed")
        .list
}

/// The copy still waits for its answer: what ran did not wait behind it.
fn still_asking(engine: &Engine, copy: JobId) {
    let state = engine.ops().job(copy).expect("known").state;
    assert!(!state.is_finished(), "{state:?}");
}

fn tree() -> tempfile::TempDir {
    test_support::tree(&[("a.txt", "a"), ("hold/a.txt", "there"), ("out/x.txt", "x")])
}

fn pack(root: &Path) -> Command {
    Command::ArchivePack {
        panel: None,
        sources: vec![root.join("out").join("x.txt").display().to_string()],
        target: Some(root.join("out").join("x.zip").display().to_string()),
        settings: PackSettings::default(),
        options: JobOptions::default(),
    }
}

#[tokio::test]
async fn packing_and_unpacking_do_not_wait_behind_a_copy() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let copy = holder(&engine, root.path()).await;
    let packed = expect_job(engine.dispatch(pack(root.path())).await.expect("packs"));
    finished(&engine, packed.id).await;
    assert_eq!(list_of(&engine, packed.id), AT_ONCE);
    let unpacked = expect_job(
        engine
            .dispatch(Command::ArchiveExtract {
                panel: None,
                archives: vec![root.path().join("out").join("x.zip").display().to_string()],
                target: Some(root.path().join("unpacked").display().to_string()),
                settings: rc_ops::ExtractSettings::default(),
                options: JobOptions::default(),
            })
            .await
            .expect("unpacks"),
    );
    finished(&engine, unpacked.id).await;
    assert_eq!(list_of(&engine, unpacked.id), AT_ONCE);
    assert_eq!(
        fs::read(root.path().join("unpacked").join("x.txt")).expect("unpacked"),
        b"x"
    );
    still_asking(&engine, copy);
}

#[tokio::test]
async fn a_checksum_job_does_not_wait_behind_a_copy() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let copy = holder(&engine, root.path()).await;
    let created = expect_job(
        engine
            .dispatch(Command::QueueChecksumJob(ChecksumJob::Create(
                ChecksumFilesRequest {
                    panel: None,
                    dir: Some(root.path().join("out").display().to_string()),
                    names: vec!["x.txt".into()],
                    algorithm: Algorithm::Sha256,
                    per_file: true,
                    file_name: None,
                },
            )))
            .await
            .expect("queued"),
    );
    finished(&engine, created.id).await;
    assert_eq!(list_of(&engine, created.id), AT_ONCE);
    assert!(root.path().join("out").join("x.txt.sha256").is_file());
    still_asking(&engine, copy);
}

#[tokio::test]
async fn a_multi_rename_does_not_wait_behind_a_copy() {
    let root = tree();
    let engine = test_support::headless_with_db(root.path()).await;
    let copy = holder(&engine, root.path()).await;
    let request = MultiRenameRequest {
        dir: Some(root.path().join("out").display().to_string()),
        names: vec!["x.txt".into()],
        rule: RenameRule {
            search: Some(SearchReplace {
                find: "x".into(),
                replace: "z".into(),
                case_sensitive: true,
                ..SearchReplace::default()
            }),
            ..RenameRule::default()
        },
        ..MultiRenameRequest::default()
    };
    let outcome = engine
        .dispatch(Command::MultiRenameApply(request))
        .await
        .expect("renames");
    let Outcome::RenameRun { job, .. } = outcome else {
        panic!("a rename run, got {outcome:?}");
    };
    finished(&engine, job.id).await;
    assert_eq!(list_of(&engine, job.id), AT_ONCE);
    assert!(root.path().join("out").join("z.txt").is_file());
    still_asking(&engine, copy);
}

#[tokio::test]
async fn a_sync_runs_at_once_as_one_batch_in_its_order() {
    // One copy each way: two jobs.
    let root = test_support::tree(&[
        ("a.txt", "a"),
        ("hold/a.txt", "there"),
        ("left/one.txt", "1"),
        ("right/two.txt", "2"),
    ]);
    let engine = test_support::open(&root.path().join("left"), &root.path().join("right")).await;
    let copy = holder(&engine, root.path()).await;
    let jobs = expect_jobs(
        engine
            .dispatch(Command::SyncApply(SyncApplyRequest::default()))
            .await
            .expect("syncs"),
    );
    assert_eq!(jobs.len(), 2, "{jobs:?}");
    assert!(jobs[0].batch.is_some() && jobs[0].batch == jobs[1].batch);
    for job in &jobs {
        finished(&engine, job.id).await;
        assert_eq!(list_of(&engine, job.id), AT_ONCE);
    }
    assert!(root.path().join("right").join("one.txt").is_file());
    assert!(root.path().join("left").join("two.txt").is_file());
    still_asking(&engine, copy);
}

#[tokio::test]
async fn switched_off_they_wait_in_the_default_list() {
    let root = tree();
    let engine = test_support::headless_with_db(root.path()).await;
    engine
        .dispatch(Command::SetSetting {
            key: TOOLS_AT_ONCE_KEY.into(),
            value: json!(false),
        })
        .await
        .expect("set");
    let copy = holder(&engine, root.path()).await;
    let packed = expect_job(engine.dispatch(pack(root.path())).await.expect("packs"));
    assert_eq!(list_of(&engine, packed.id), DEFAULT_LIST);
    let waited = tokio::time::timeout(Duration::from_millis(300), engine.ops().wait(packed.id));
    assert!(waited.await.is_err(), "behind the copy in its lane");
    still_asking(&engine, copy);
}
