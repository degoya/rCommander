use std::time::Duration;

use rc_db::{Db, StoredTransfer};
use rc_ops::{JobKind, JobOptions, JobSpec, JobState};
use rc_vfs::{BackendId, VfsPath};

use super::read_row;
use crate::test_support;
use crate::{Command, Engine, Outcome};

fn server(connection_id: i64, path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Sftp { connection_id },
        path: path.into(),
    }
}

fn upload(from: &str, to: VfsPath) -> String {
    let spec = JobSpec {
        kind: JobKind::Copy {
            sources: vec![VfsPath::local(from)],
            target: to,
        },
        options: JobOptions::default(),
    };
    serde_json::to_string(&spec).expect("json")
}

fn row(spec: String, file: Option<&str>) -> StoredTransfer {
    StoredTransfer {
        id: 3,
        recorded_at: "2026-10-18T10:00:00.000Z".to_owned(),
        spec,
        file: file.map(str::to_owned),
    }
}

/// An engine on a temporary folder with `db` attached: the next start of the app.
async fn start(db: &Db) -> (tempfile::TempDir, Engine) {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = test_support::open(root.path(), root.path()).await;
    assert!(engine.attach_db(db.clone()));
    (root, engine)
}

#[test]
fn a_row_reads_only_as_a_transfer_on_this_machine_or_a_saved_connection_with_plain_paths() {
    let good = row(upload("/home/a.bin", server(5, "/up")), None);
    let (saved, batch) = read_row(&good).expect("reads");
    assert_eq!(saved.record, 3);
    assert!(saved.mark.is_none());
    assert_eq!(batch, None);

    let mark =
        r#"{"source":"/home/a.bin","target":"/up/a.bin","size":9,"modified_ns":1,"replace":false}"#;
    let marked = read_row(&row(upload("/home/a.bin", server(5, "/up")), Some(mark)));
    assert_eq!(
        marked
            .and_then(|(saved, _)| saved.mark)
            .map(|mark| mark.size),
        Some(9)
    );

    // RC-080-04: a copy from disk to disk is kept too.
    let local = read_row(&row(upload("/home/a.bin", VfsPath::local("/b")), None));
    assert_eq!(local.map(|(saved, _)| saved.record), Some(3));

    for bad in [
        row("{}".to_owned(), None),
        row("not json".to_owned(), None),
        // A quick connection's id means nothing after a restart.
        row(upload("/home/a.bin", server(-1, "/up")), None),
        row(upload("/home/../etc/a", server(5, "/up")), None),
        row(upload("home/a.bin", server(5, "/up")), None),
        row(upload("/home/a.bin", server(5, "/up")), Some("{\"broken\"")),
    ] {
        assert!(read_row(&bad).is_none(), "{bad:?}");
    }
    let delete = JobSpec {
        kind: JobKind::Delete {
            sources: vec![server(5, "/up/a")],
            mode: rc_ops::DeleteMode::Permanent,
        },
        options: JobOptions::default(),
    };
    let delete = row(serde_json::to_string(&delete).expect("json"), None);
    assert!(read_row(&delete).is_none(), "only transfers come back");
}

#[tokio::test]
async fn a_restart_puts_kept_jobs_back_paused_and_drops_what_does_not_read() {
    let db = Db::open_in_memory().await.expect("db");
    // What the last run left: one upload in the middle, and rows that must not run.
    let kept = db
        .save_transfer(&upload("/home/a.bin", server(5, "/up")))
        .await
        .expect("save");
    db.save_transfer(&upload("/home/../x", server(5, "/up")))
        .await
        .expect("save");
    db.save_transfer("{}").await.expect("save");

    let (_root, engine) = start(&db).await;
    let restored = engine.restore_transfers().await.expect("restore");
    assert_eq!(restored.len(), 1);
    let job = &restored[0];
    assert_eq!(job.state, JobState::Paused);
    assert!(job.resumable);
    let rows: Vec<i64> = db
        .transfers()
        .await
        .expect("rows")
        .iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(rows, [kept], "the rows that do not read are dropped");

    // `ops.list` shows it resumable, as MCP sees it.
    let listed = engine
        .dispatch(Command::OpsList { job: Some(job.id) })
        .await
        .expect("list");
    let Outcome::Jobs { jobs } = listed else {
        panic!("jobs, got {listed:?}");
    };
    assert!(jobs[0].resumable);

    // Cancelling discards it, its row too.
    engine
        .dispatch(Command::OpsCancel { job: job.id })
        .await
        .expect("cancel");
    let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
        .await
        .expect("in time")
        .expect("known");
    assert_eq!(done.state, JobState::Cancelled);
    assert!(db.transfers().await.expect("rows").is_empty());
}

/// `spec` as the store keeps a job of the batch `batch` (RC-010-14).
fn in_batch(spec: &str, batch: u64) -> String {
    let mut value: serde_json::Value = serde_json::from_str(spec).expect("json");
    value["batch"] = batch.into();
    value.to_string()
}

#[tokio::test]
async fn the_jobs_of_a_kept_batch_come_back_as_siblings_in_their_order() {
    let db = Db::open_in_memory().await.expect("db");
    let a = upload("/home/a.bin", server(5, "/up"));
    let b = upload("/home/b.bin", server(5, "/up"));
    let other = upload("/home/other.bin", server(5, "/up"));
    db.save_transfer(&in_batch(&a, 40)).await.expect("save");
    db.save_transfer(&other).await.expect("save");
    db.save_transfer(&in_batch(&b, 40)).await.expect("save");
    assert_eq!(
        read_row(&row(in_batch(&a, 40), None)).map(|(_, batch)| batch),
        Some(Some(40))
    );

    let (_root, engine) = start(&db).await;
    let restored = engine.restore_transfers().await.expect("restore");
    let sources: Vec<String> = restored
        .iter()
        .map(|job| match &job.kind {
            JobKind::Copy { sources, .. } => sources[0].path.display().to_string(),
            other => panic!("a copy, got {other:?}"),
        })
        .collect();
    assert_eq!(sources, ["/home/a.bin", "/home/b.bin", "/home/other.bin"]);
    assert!(restored[0].batch.is_some());
    assert_eq!(restored[0].batch, restored[1].batch, "siblings again");
    assert_eq!(restored[2].batch, None);
    assert!(restored.iter().all(|job| job.state == JobState::Paused));
}

#[tokio::test]
async fn without_a_database_nothing_is_restored() {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = test_support::open(root.path(), root.path()).await;
    assert!(engine.restore_transfers().await.is_err());
}
