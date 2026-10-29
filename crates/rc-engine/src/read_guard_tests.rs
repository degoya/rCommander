use std::path::Path;

use rc_vfs::BackendId;

use super::{MCP, ReadGuard, current, from_mcp};

/// A test judge: every file whose name starts with `secret`.
fn secrets(path: &Path) -> Option<&'static str> {
    path.file_name()?
        .to_string_lossy()
        .starts_with("secret")
        .then_some("a test secret")
}

#[tokio::test]
async fn there_is_a_guard_inside_an_mcp_dispatch_only() {
    assert!(
        current(&BackendId::Local).is_none(),
        "the window reads freely"
    );
    assert!(!from_mcp());
    let inside = MCP
        .scope(Some(secrets), async {
            current(&BackendId::Local).map(|guard| guard.refuses(Path::new("/x/secret.txt")))
        })
        .await;
    assert_eq!(inside, Some(Some("a test secret")));
    let unjudged = MCP
        .scope(None, async {
            (from_mcp(), current(&BackendId::Local).is_none())
        })
        .await;
    assert_eq!(unjudged, (true, true), "an MCP dispatch without a judge");
}

#[test]
fn a_file_is_judged_by_its_name_when_nothing_is_there() {
    let guard = ReadGuard::new(secrets, &BackendId::Local);
    assert_eq!(
        guard.refuses(Path::new("/no/such/secret.key")),
        Some("a test secret")
    );
    assert_eq!(guard.refuses(Path::new("/no/such/plain.txt")), None);
}

/// A link with a harmless name is judged by the file it leads to.
#[cfg(unix)]
#[tokio::test]
async fn a_link_is_judged_by_where_it_leads() {
    let root = tempfile::tempdir().expect("temp dir");
    let target = root.path().join("secret.txt");
    std::fs::write(&target, "x").expect("write");
    let link = root.path().join("plain.txt");
    std::os::unix::fs::symlink(&target, &link).expect("link");
    let guard = ReadGuard::new(secrets, &BackendId::Local);
    assert_eq!(guard.refuses(&link), Some("a test secret"));
    let error = guard.check(&link).await.expect_err("refused");
    assert_eq!(error.code(), "sensitive_path");
}

/// RC-080-09: a copy of the database under a harmless name is refused by its content, on the
/// local backend; another file with SQLite's header is not.
#[tokio::test]
async fn a_database_copy_is_known_by_its_content() {
    let root = tempfile::tempdir().expect("temp dir");
    let copy = root.path().join("holiday-photos.bin");
    let db = rc_db::Db::open(&copy).await.expect("db");
    db.close().await;
    let guard = ReadGuard::new(secrets, &BackendId::Local);
    assert_eq!(guard.refuses(&copy), Some("rCommander's own data"));
    let error = guard.check(&copy).await.expect_err("refused");
    assert_eq!(error.code(), "sensitive_path");

    let other = root.path().join("other.sqlite");
    let mut bytes = b"SQLite format 3\0".to_vec();
    bytes.extend_from_slice(&[0_u8; 4096]);
    std::fs::write(&other, bytes).expect("write");
    assert_eq!(guard.refuses(&other), None);
    let text = root.path().join("notes.txt");
    std::fs::write(&text, "vault_meta").expect("write");
    assert_eq!(
        guard.refuses(&text),
        None,
        "only a file with SQLite's header"
    );
}

/// RC-080-42: a copy an MCP call submits runs under the read guard, the window's does not; a
/// list loaded through MCP refuses, when it runs, a file that became sensitive after the load -
/// that file fails as `sensitive_path`, the others are copied.
#[tokio::test]
async fn an_mcp_job_is_guarded_while_it_runs() {
    use std::sync::atomic::{AtomicBool, Ordering};

    use rc_ops::{ConflictChoice, ErrorPolicy, JobKind, JobOptions, JobSpec, JobState};
    use rc_vfs::VfsPath;

    use crate::{Command, Outcome, TransferRequest, test_support};

    static LATE: AtomicBool = AtomicBool::new(false);
    fn late(path: &Path) -> Option<&'static str> {
        (LATE.load(Ordering::SeqCst) && path.file_name()? == "b.txt").then_some("a late key")
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::write(root.join("src").join("a.txt"), b"a").expect("write");
    std::fs::write(root.join("src").join("b.txt"), b"b").expect("write");
    let engine = test_support::engine_in(root).await;
    assert!(engine.attach_read_judge(late));
    let options = JobOptions {
        on_conflict: Some(ConflictChoice::Skip),
        on_error: ErrorPolicy::Skip,
        ..JobOptions::default()
    };
    let copy = |target: &str| {
        Command::OpsCopy(TransferRequest {
            panel: None,
            sources: vec![root.join("src").display().to_string()],
            target: Some(root.join(target).display().to_string()),
            options,
        })
    };
    let wait = |id| {
        let engine = engine.clone();
        async move {
            tokio::time::timeout(std::time::Duration::from_secs(10), engine.ops().wait(id))
                .await
                .expect("in time")
                .expect("known")
        }
    };
    let Ok(Outcome::Job(mcp)) = engine.dispatch_mcp(copy("first")).await else {
        panic!("one job");
    };
    assert!(mcp.options.read_guarded, "submitted by MCP");
    let Ok(Outcome::Job(window)) = engine.dispatch(copy("second")).await else {
        panic!("one job");
    };
    assert!(!window.options.read_guarded, "the window's");
    assert_eq!(wait(mcp.id).await.state, JobState::Completed);
    assert_eq!(wait(window.id).await.state, JobState::Completed);

    let spec = JobSpec {
        kind: JobKind::Copy {
            sources: vec![VfsPath::local(root.join("src"))],
            target: VfsPath::local(root.join("out")),
        },
        options,
    };
    let text = serde_json::to_string(&spec).expect("json");
    let saved = engine
        .db()
        .expect("a database")
        .save_queue("Later", false, &[text])
        .await
        .expect("saved");
    let Ok(Outcome::QueueLoaded { list, jobs, .. }) = engine
        .dispatch_mcp(Command::QueueLoad { saved: saved.id })
        .await
    else {
        panic!("loaded");
    };
    assert!(jobs.iter().all(|job| job.options.read_guarded), "{jobs:?}");
    // A key by the time the job opens it.
    LATE.store(true, Ordering::SeqCst);
    engine
        .dispatch(Command::QueueStart { list: list.id })
        .await
        .expect("started");
    let done = wait(jobs[0].id).await;
    LATE.store(false, Ordering::SeqCst);
    let failures: Vec<&str> = done
        .failures
        .iter()
        .map(|failure| failure.code.as_str())
        .collect();
    assert_eq!(failures, ["sensitive_path"], "{done:?}");
    assert!(root.join("out").join("src").join("a.txt").exists());
    assert!(!root.join("out").join("src").join("b.txt").exists());
}

/// Many paths are judged in one go, in their order: the first refused is named, on the local
/// backend and on a server's alike.
#[tokio::test]
async fn a_batch_names_its_first_refused_path() {
    for backend in [BackendId::Local, BackendId::Memory(1)] {
        let guard = ReadGuard::new(secrets, &backend);
        let plain: Vec<_> = (0..3)
            .map(|at| Path::new("/no/such").join(format!("plain{at}.txt")))
            .collect();
        guard.check_all(plain.clone()).await.expect("all plain");
        guard.check_all(Vec::new()).await.expect("nothing to judge");
        let mut mixed = plain;
        mixed.insert(1, Path::new("/no/such/secret-b").to_path_buf());
        mixed.push(Path::new("/no/such/secret-z").to_path_buf());
        let error = guard.check_all(mixed).await.expect_err("refused");
        assert_eq!(error.code(), "sensitive_path");
        assert!(error.to_string().contains("secret-b"), "{error}");
    }
}
