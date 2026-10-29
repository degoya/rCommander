use std::{fs, time::Duration};

use rc_ops::{ConflictChoice, JobOptions, JobState};

use crate::test_support;
use crate::test_support::{expect_job, expect_jobs};
use crate::{Command, Engine, EngineEvent, JobChange, Outcome, PanelId};

async fn engine() -> (tempfile::TempDir, Engine) {
    let root = test_support::tree(&[("docs/", ""), ("out/", ""), ("a.txt", "a"), ("b.txt", "b")]);
    let engine = test_support::open(root.path(), &root.path().join("out")).await;
    (root, engine)
}

async fn finished(engine: &Engine, id: rc_ops::JobId) -> rc_ops::JobSnapshot {
    tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(id))
        .await
        .expect("in time")
        .expect("known")
}

#[tokio::test]
async fn copy_takes_the_selection_into_the_other_panel() {
    let (root, engine) = engine().await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: vec!["a.txt".into(), "docs".into()],
        })
        .await
        .expect("selects");
    let jobs = expect_jobs(
        engine
            .dispatch(Command::OpsCopy(crate::TransferRequest {
                panel: None,
                sources: Vec::new(),
                target: None,
                options: JobOptions::default(),
            }))
            .await
            .expect("queued"),
    );
    // `queue.one_job_per_item` is on by default: one job per selected entry, one batch.
    assert_eq!(jobs.len(), 2, "{jobs:?}");
    assert!(jobs[0].batch.is_some() && jobs[0].batch == jobs[1].batch);
    for job in &jobs {
        let done = finished(&engine, job.id).await;
        assert_eq!(done.state, JobState::Completed, "{done:?}");
    }
    assert_eq!(
        fs::read(root.path().join("out").join("a.txt")).expect("copied"),
        b"a"
    );
    assert!(root.path().join("out").join("docs").is_dir());
    assert!(
        !root.path().join("out").join("b.txt").exists(),
        "not selected"
    );

    let mut finished_seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let EngineEvent::Job {
            job: id,
            change: JobChange::Finished { state },
        } = event
        {
            assert_eq!(state, JobState::Completed);
            finished_seen.push(id);
        }
    }
    assert_eq!(
        finished_seen,
        [jobs[0].id, jobs[1].id],
        "the queue's events reach the engine's stream"
    );
}

/// `queue.one_job_per_item` off: a copy of the selection is one job, as before; on, a delete of
/// two entries is two jobs of one batch, in the selection's order.
#[tokio::test]
async fn one_job_per_item_follows_its_setting() {
    let (root, engine) = engine().await;
    assert!(engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")));
    let select = Command::ToggleSelection {
        panel: None,
        names: vec!["a.txt".into(), "b.txt".into()],
    };
    engine.dispatch(select).await.expect("selects");
    let switch = |on: bool| Command::SetSetting {
        key: crate::settings::ONE_JOB_PER_ITEM_KEY.into(),
        value: serde_json::json!(on),
    };
    engine.dispatch(switch(false)).await.expect("stored");
    let copy = Command::OpsCopy(crate::TransferRequest {
        panel: None,
        sources: Vec::new(),
        target: None,
        options: JobOptions::default(),
    });
    let one = expect_job(engine.dispatch(copy).await.expect("queued"));
    assert_eq!(one.batch, None);
    assert!(
        matches!(&one.kind, rc_ops::JobKind::Copy { sources, .. } if sources.len() == 2),
        "{one:?}"
    );
    assert_eq!(finished(&engine, one.id).await.state, JobState::Completed);

    engine.dispatch(switch(true)).await.expect("stored");
    let delete = Command::OpsDelete(crate::DeleteRequest {
        panel: None,
        sources: vec!["a.txt".into(), "b.txt".into()],
        mode: rc_ops::DeleteMode::Permanent,
        on_error: rc_ops::ErrorPolicy::Skip,
        queue: None,
    });
    let jobs = expect_jobs(engine.dispatch(delete).await.expect("queued"));
    let names: Vec<_> = jobs
        .iter()
        .map(|job| match &job.kind {
            rc_ops::JobKind::Delete { sources, .. } => sources[0].path.clone(),
            other => panic!("a delete, got {other:?}"),
        })
        .collect();
    assert_eq!(
        names,
        [root.path().join("a.txt"), root.path().join("b.txt")]
    );
    for job in &jobs {
        assert_eq!(finished(&engine, job.id).await.state, JobState::Completed);
    }
    assert!(!root.path().join("a.txt").exists() && !root.path().join("b.txt").exists());
}

#[tokio::test]
async fn named_sources_and_a_relative_target() {
    let (root, engine) = engine().await;
    let job = expect_job(
        engine
            .dispatch(Command::OpsMove(crate::TransferRequest {
                panel: Some(PanelId::Left),
                sources: vec!["b.txt".into()],
                target: Some("../docs".into()),
                options: JobOptions {
                    on_conflict: Some(ConflictChoice::Skip),
                    ..JobOptions::default()
                },
            }))
            .await
            .expect("queued"),
    );
    let done = finished(&engine, job.id).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(root.path().join("docs").join("b.txt").exists());
    assert!(!root.path().join("b.txt").exists());
}

#[tokio::test]
async fn mkdir_list_and_a_finished_job() {
    let (root, engine) = engine().await;
    let job = expect_job(
        engine
            .dispatch(Command::OpsMkdir {
                panel: None,
                path: "new/deeper".into(),
            })
            .await
            .expect("queued"),
    );
    finished(&engine, job.id).await;
    assert!(root.path().join("new").join("deeper").is_dir());
    let Outcome::Jobs { jobs } = engine
        .dispatch(Command::OpsList { job: None })
        .await
        .expect("lists")
    else {
        panic!("jobs");
    };
    assert_eq!(jobs.len(), 1);
    let late = engine
        .dispatch(Command::OpsPause { job: job.id })
        .await
        .expect_err("finished");
    assert_eq!(late.code(), "job_finished");
    let unknown = engine
        .dispatch(Command::OpsList { job: Some(999) })
        .await
        .expect_err("unknown");
    assert_eq!(unknown.code(), "job_not_found");
}

#[tokio::test]
async fn nothing_under_the_cursor_is_no_sources() {
    let (_root, engine) = test_support::engine(&[]).await;
    let refused = engine
        .dispatch(Command::OpsDelete(crate::DeleteRequest {
            panel: None,
            sources: Vec::new(),
            mode: rc_ops::DeleteMode::Permanent,
            on_error: rc_ops::ErrorPolicy::Ask,
            queue: None,
        }))
        .await
        .expect_err("empty panel");
    assert_eq!(refused.code(), "no_sources");
}

/// A test read judge: every file named `credentials`.
fn credentials(path: &std::path::Path) -> Option<&'static str> {
    (path.file_name()? == "credentials").then_some("a test secret")
}

/// Under MCP a copy or a move whose sources hold a guarded file - given, selected, or deep in a
/// folder - is refused before a job exists; the window copies it.
#[tokio::test]
async fn an_mcp_copy_of_a_folder_holding_a_guarded_file_is_refused() {
    let (root, engine) = engine().await;
    fs::create_dir(root.path().join("docs").join("deep")).expect("mkdir");
    fs::write(root.path().join("docs/deep/credentials"), b"key").expect("write");
    assert!(engine.attach_read_judge(credentials));
    let copy = |sources: Vec<String>| {
        Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources,
            target: None,
            options: JobOptions::default(),
        })
    };
    let error = engine
        .dispatch_mcp(copy(vec!["docs".into()]))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "sensitive_path");
    engine
        .dispatch(Command::ToggleSelection {
            panel: None,
            names: vec!["a.txt".into(), "docs".into()],
        })
        .await
        .expect("selects");
    let moving = Command::OpsMove(crate::TransferRequest {
        panel: None,
        sources: Vec::new(),
        target: None,
        options: JobOptions::default(),
    });
    let error = engine
        .dispatch_mcp(moving)
        .await
        .expect_err("the selection is refused too");
    assert_eq!(error.code(), "sensitive_path");
    assert!(engine.ops().jobs().is_empty(), "no job was queued");
    let job = expect_job(
        engine
            .dispatch(copy(vec!["docs".into()]))
            .await
            .expect("the window copies it"),
    );
    assert_eq!(finished(&engine, job.id).await.state, JobState::Completed);
    let plain = expect_job(
        engine
            .dispatch_mcp(copy(vec!["b.txt".into()]))
            .await
            .expect("a plain file is copied"),
    );
    assert_eq!(finished(&engine, plain.id).await.state, JobState::Completed);
}

/// `ops.copy`, `ops.move_to` and their sources reach a server written as
/// `sftp://<connection>/<path>`, and a copy hands the `transfers.*` settings to the queue
/// (RC-060-05, the retries of RC-060-07).
#[tokio::test]
async fn copies_and_moves_reach_a_server_by_its_location() {
    let harness = crate::remote::sftp::tests::harness().await;
    let engine = &harness.engine;
    let served = harness.served.path().join("home/tester");
    let here = tempfile::tempdir().expect("temp dir");
    let file = here.path().join("up.txt");
    fs::write(&file, b"up").expect("write");
    engine
        .dispatch(Command::SetSetting {
            key: "transfers.per_connection".to_owned(),
            value: serde_json::json!(3),
        })
        .await
        .expect("stored");
    engine
        .dispatch(Command::SetSetting {
            key: "transfers.retry_count".to_owned(),
            value: serde_json::json!(5),
        })
        .await
        .expect("stored");

    let copy = |sources: Vec<String>, target: String| {
        Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources,
            target: Some(target),
            options: JobOptions::default(),
        })
    };
    let up = copy(
        vec![file.display().to_string()],
        "sftp://box/home/tester/in".to_owned(),
    );
    let up = expect_job(engine.dispatch(up).await.expect("queued"));
    assert_eq!(finished(engine, up.id).await.state, JobState::Completed);
    assert_eq!(fs::read(served.join("in/up.txt")).expect("uploaded"), b"up");
    assert_eq!(
        engine.ops().config().per_connection,
        3,
        "the setting counts"
    );
    assert_eq!(engine.ops().config().retry.attempts, 5);

    let back = here.path().join("back");
    let down = copy(
        vec!["sftp://box/home/tester/readme.txt".to_owned()],
        back.display().to_string(),
    );
    let down = expect_job(engine.dispatch(down).await.expect("queued"));
    assert_eq!(finished(engine, down.id).await.state, JobState::Completed);
    assert_eq!(
        fs::read(back.join("readme.txt")).expect("downloaded"),
        b"hello"
    );

    let moved = Command::OpsMoveTo {
        panel: None,
        sources: vec![file.display().to_string()],
        target: "sftp://box/home/tester/moved/".to_owned(),
        options: JobOptions::default(),
        queue: None,
    };
    let moved = expect_job(engine.dispatch(moved).await.expect("queued"));
    assert_eq!(finished(engine, moved.id).await.state, JobState::Completed);
    assert!(served.join("moved/up.txt").is_file());
    assert!(!file.exists(), "the source went once the copy was there");
}

/// One call's sources on two backends - a local file and one at a server's location - are each
/// copied: every source's backend is opened for the queue, not only the first one's
/// (RC-080-50).
#[tokio::test]
async fn sources_on_the_local_disk_and_a_server_copy_in_one_call() {
    let harness = crate::remote::sftp::tests::harness().await;
    let engine = &harness.engine;
    let here = tempfile::tempdir().expect("temp dir");
    let file = here.path().join("up.txt");
    fs::write(&file, b"up").expect("write");
    let out = here.path().join("out");
    fs::create_dir(&out).expect("mkdir");
    let queued = engine
        .dispatch(Command::OpsCopy(crate::TransferRequest {
            panel: None,
            sources: vec![
                file.display().to_string(),
                "sftp://box/home/tester/readme.txt".to_owned(),
            ],
            target: Some(out.display().to_string()),
            options: JobOptions::default(),
        }))
        .await
        .expect("queued");
    let jobs = expect_jobs(queued);
    assert_eq!(jobs.len(), 2, "one job per source");
    for job in jobs {
        assert_eq!(finished(engine, job.id).await.state, JobState::Completed);
    }
    assert_eq!(fs::read(out.join("up.txt")).expect("copied"), b"up");
    assert_eq!(
        fs::read(out.join("readme.txt")).expect("downloaded"),
        b"hello"
    );
}

/// A new folder is made at once, as OK in the other dialogs: it does not wait in the drive's
/// lane behind a copy (owner 2026-10-25: F7 did nothing while a large copy ran). The copy holds
/// the lane while it waits for an answer to a conflict.
#[tokio::test]
async fn mkdir_does_not_wait_behind_a_copy_in_the_lane() {
    let (root, engine) = engine().await;
    fs::write(root.path().join("out").join("a.txt"), "there").expect("conflict");
    let copy = expect_job(
        engine
            .dispatch(Command::OpsCopyTo {
                panel: None,
                sources: vec!["a.txt".into()],
                target: root.path().join("out").display().to_string(),
                options: JobOptions::default(),
                queue: Some(rc_ops::DEFAULT_LIST),
            })
            .await
            .expect("queued"),
    );
    let made = expect_job(
        engine
            .dispatch(Command::OpsMkdir {
                panel: None,
                path: "now".into(),
            })
            .await
            .expect("made"),
    );
    finished(&engine, made.id).await;
    assert!(root.path().join("now").is_dir());
    assert!(
        !engine
            .ops()
            .job(copy.id)
            .expect("known")
            .state
            .is_finished()
    );
}
