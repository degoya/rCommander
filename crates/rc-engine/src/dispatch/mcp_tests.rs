use rc_ops::{ConflictChoice, JobKind, JobOptions, JobSpec};
use rc_vfs::VfsPath;

use crate::{Command, CommandClass, RetentionRequest, test_support, with_mcp_judged};

fn retention(table: &str, max_rows: Option<u64>, max_age_days: Option<u32>) -> Command {
    Command::SetRetention(RetentionRequest {
        table: table.into(),
        max_rows,
        max_age_days,
    })
}

/// RC-080-12 (M10), now the engine's: a limit below the one in force, or limits that cannot be
/// read, are destructive; never a raise.
#[tokio::test]
async fn only_a_lowered_retention_is_destructive() {
    let (_dir, engine) = test_support::engine_with_db(&[]).await;
    for (command, expected) in [
        (
            retention("error_log", Some(4_999), None),
            CommandClass::Destructive,
        ),
        (
            retention("error_log", None, Some(29)),
            CommandClass::Destructive,
        ),
        (
            retention("rename_log", Some(10), None),
            CommandClass::Destructive,
        ),
        (
            retention("nowhere", Some(100), None),
            CommandClass::Destructive,
        ),
        (
            retention("error_log", Some(5_000), None),
            CommandClass::Write,
        ),
        (
            retention("error_log", Some(9_000), Some(90)),
            CommandClass::Write,
        ),
        (retention("error_log", None, None), CommandClass::Write),
    ] {
        assert_eq!(
            engine.effective_class(&command).await,
            expected,
            "{command:?}"
        );
    }
    let (_dir, without) = test_support::engine(&[]).await;
    assert_eq!(
        without
            .effective_class(&retention("error_log", Some(9_000), None))
            .await,
        CommandClass::Destructive,
        "limits that cannot be read"
    );
}

/// A saved list is loaded as its jobs would be queued: one that overwrites makes the load
/// destructive.
#[tokio::test]
async fn loading_a_list_with_an_overwriting_job_is_destructive() {
    let (dir, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    let job = |on_conflict| {
        serde_json::to_string(&JobSpec {
            kind: JobKind::Copy {
                sources: vec![VfsPath::local(dir.path().join("a.txt"))],
                target: VfsPath::local(dir.path().join("to")),
            },
            options: JobOptions {
                on_conflict,
                ..JobOptions::default()
            },
        })
        .expect("serializes")
    };
    let db = engine.db().expect("a database");
    let plain = db
        .save_queue(
            "plain",
            false,
            &[job(Some(ConflictChoice::Skip)), job(None)],
        )
        .await
        .expect("saved");
    let overwriting = db
        .save_queue(
            "overwriting",
            false,
            &[
                job(Some(ConflictChoice::Skip)),
                job(Some(ConflictChoice::OverwriteLarger)),
            ],
        )
        .await
        .expect("saved");
    let load = |saved: i64| Command::QueueLoad { saved };
    assert_eq!(
        engine.effective_class(&load(plain.id)).await,
        CommandClass::Write
    );
    assert_eq!(
        engine.effective_class(&load(overwriting.id)).await,
        CommandClass::Destructive
    );
    assert_eq!(
        engine.effective_class(&load(9_999)).await,
        CommandClass::Write,
        "a list that is not there fails with saved_queue_not_found"
    );
}

/// The engine's class is the command's own for every other call.
#[tokio::test]
async fn other_calls_keep_the_class_of_their_parameters() {
    let (_dir, engine) = test_support::engine_with_db(&[]).await;
    let command = Command::OpsMkdir {
        panel: None,
        path: "new".into(),
    };
    assert_eq!(engine.effective_class(&command).await, CommandClass::Write);
}

/// RC-080-66: the class of `queue.load` is bound to the list it was judged on - a list saved
/// again under the same id in between, now with a job that overwrites, is refused with
/// `queue_changed`; the list judged loads.
#[tokio::test]
async fn a_list_changed_after_its_class_was_judged_is_not_loaded() {
    let (dir, engine) = test_support::engine_with_db(&[("a.txt", "a")]).await;
    let job = |on_conflict| {
        serde_json::to_string(&JobSpec {
            kind: JobKind::Copy {
                sources: vec![VfsPath::local(dir.path().join("a.txt"))],
                target: VfsPath::local(dir.path().join("to")),
            },
            options: JobOptions {
                on_conflict,
                ..JobOptions::default()
            },
        })
        .expect("serializes")
    };
    let db = engine.db().expect("a database");
    let saved = db
        .save_queue("list", false, &[job(Some(ConflictChoice::Skip))])
        .await
        .expect("saved");
    let load = Command::QueueLoad { saved: saved.id };
    let (class, judged) = engine.judge_class(&load).await;
    assert_eq!(class, CommandClass::Write);

    let swapped = db
        .save_queue("list", false, &[job(Some(ConflictChoice::Overwrite))])
        .await
        .expect("saved again");
    assert_eq!(swapped.id, saved.id, "saved again under the same id");
    let refused = with_mcp_judged(judged, engine.dispatch_mcp(load.clone())).await;
    assert_eq!(
        refused.expect_err("refused").code(),
        "queue_changed",
        "the list changed after its class was judged"
    );

    let (class, judged) = engine.judge_class(&load).await;
    assert_eq!(class, CommandClass::Destructive);
    let loaded = with_mcp_judged(judged, engine.dispatch_mcp(load)).await;
    assert!(loaded.is_ok(), "the list judged loads: {loaded:?}");
    for job in engine.ops().jobs() {
        let _ = engine.ops().cancel(job.id);
    }
}
