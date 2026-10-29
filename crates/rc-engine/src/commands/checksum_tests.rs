use crate::test_support;
use crate::test_support::expect_job;
use crate::{Algorithm, Command, Engine, Outcome, VerifyStatus, VfsPath};

const SHA256_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

async fn engine() -> (tempfile::TempDir, Engine) {
    test_support::engine(&[("a.txt", "abc"), ("b.txt", "abc")]).await
}

#[tokio::test]
async fn compute_answers_each_file_and_codes_the_failures() {
    let (root, engine) = engine().await;
    let absolute = root.path().join("b.txt").to_string_lossy().into_owned();
    let outcome = engine
        .dispatch(Command::ComputeChecksums(crate::ChecksumRequest {
            panel: None,
            paths: vec!["a.txt".into(), absolute, "nope".into()],
            algorithm: Algorithm::Sha256,
        }))
        .await
        .expect("computed");
    let Outcome::Checksums { algorithm, files } = outcome else {
        panic!("checksums expected");
    };
    assert_eq!(algorithm, Algorithm::Sha256);
    assert_eq!(files[0].path, VfsPath::local(root.path().join("a.txt")));
    assert_eq!(files[0].hex.as_deref(), Some(SHA256_ABC));
    assert_eq!(files[1].hex.as_deref(), Some(SHA256_ABC));
    assert_eq!(files[2].hex, None);
    assert_eq!(files[2].code.as_deref(), Some("not_found"));
    let none = engine
        .dispatch(Command::ComputeChecksums(crate::ChecksumRequest {
            panel: None,
            paths: Vec::new(),
            algorithm: Algorithm::Md5,
        }))
        .await
        .expect_err("nothing");
    assert_eq!(none.code(), "checksum_no_files");
}

#[tokio::test]
async fn create_then_verify_through_the_panel_directory() {
    let (root, engine) = engine().await;
    let created = engine
        .dispatch(Command::CreateChecksumFiles(crate::ChecksumFilesRequest {
            panel: None,
            dir: None,
            names: vec!["a.txt".into(), "b.txt".into()],
            algorithm: Algorithm::Sha256,
            per_file: false,
            file_name: None,
        }))
        .await
        .expect("created");
    let Outcome::ChecksumsCreated { dir, report } = created else {
        panic!("created expected");
    };
    assert_eq!(dir, VfsPath::local(root.path()));
    assert_eq!(report.written, vec!["checksums.sha256"]);
    std::fs::write(root.path().join("b.txt"), b"abd").expect("tamper");
    let verified = engine
        .dispatch(Command::VerifyChecksumFile {
            panel: None,
            path: "checksums.sha256".into(),
        })
        .await
        .expect("verified");
    let Outcome::ChecksumsVerified { report, .. } = verified else {
        panic!("verified expected");
    };
    let statuses: Vec<VerifyStatus> = report.entries.iter().map(|entry| entry.status).collect();
    assert_eq!(statuses, vec![VerifyStatus::Ok, VerifyStatus::Mismatch]);
}

#[tokio::test]
async fn split_and_combine_round_trip_with_default_folders() {
    let (root, engine) = engine().await;
    let data: Vec<u8> = (0..5_000_u32).map(|n| (n % 251) as u8).collect();
    std::fs::write(root.path().join("data.bin"), &data).expect("data");
    std::fs::create_dir(root.path().join("back")).expect("back");
    let split = engine
        .dispatch(Command::SplitFile(crate::SplitRequest {
            panel: None,
            path: "data.bin".into(),
            part_size: 2_000,
            target_dir: None,
        }))
        .await
        .expect("split");
    let Outcome::Split { dir, report } = split else {
        panic!("split expected");
    };
    assert_eq!(dir, VfsPath::local(root.path()));
    assert_eq!(report.parts, vec!["data.001", "data.002", "data.003"]);
    let combined = engine
        .dispatch(Command::CombineFiles(crate::CombineRequest {
            panel: None,
            path: "data.001".into(),
            target_dir: Some("back".into()),
            target_name: None,
        }))
        .await
        .expect("combined");
    let Outcome::Combined { report, .. } = combined else {
        panic!("combined expected");
    };
    assert!(report.crc_checked);
    assert_eq!(
        std::fs::read(root.path().join("back").join("data.bin")).expect("back"),
        data
    );
    std::fs::remove_file(root.path().join("data.002")).expect("remove");
    let missing = engine
        .dispatch(Command::CombineFiles(crate::CombineRequest {
            panel: None,
            path: "data.crc".into(),
            target_dir: Some("back".into()),
            target_name: Some("again.bin".into()),
        }))
        .await
        .expect_err("missing");
    assert_eq!(missing.code(), "combine_part_missing");
    assert!(!root.path().join("back").join("again.bin").exists());
}

#[test]
fn a_create_command_reads_with_defaults() {
    let command: Command = serde_json::from_str(
        r#"{"command":"checksum.create","params":{"names":["a.iso"],"algorithm":"sha256"}}"#,
    )
    .expect("deserializes");
    assert_eq!(
        command,
        Command::CreateChecksumFiles(crate::ChecksumFilesRequest {
            panel: None,
            dir: None,
            names: vec!["a.iso".into()],
            algorithm: Algorithm::Sha256,
            per_file: false,
            file_name: None,
        })
    );
}

/// A test read judge: `b.txt` is secret.
fn b_is_secret(path: &std::path::Path) -> Option<&'static str> {
    (path.file_name()? == "b.txt").then_some("a test secret")
}

/// Under MCP no checksum run hashes a guarded file, however it reaches it: a folder's contents
/// fail the run before anything is written, and a checksum file's line for it reports no digest.
#[tokio::test]
async fn an_mcp_run_never_hashes_a_guarded_file() {
    let (root, engine) = engine().await;
    std::fs::create_dir(root.path().join("sub")).expect("sub");
    std::fs::write(root.path().join("sub").join("b.txt"), b"abc").expect("b");
    std::fs::write(
        root.path().join("list.sha256"),
        format!("{SHA256_ABC} *a.txt\n{SHA256_ABC} *b.txt\n"),
    )
    .expect("list");
    assert!(engine.attach_read_judge(b_is_secret));
    let error = engine
        .dispatch_mcp(Command::CreateChecksumFiles(crate::ChecksumFilesRequest {
            panel: None,
            dir: None,
            names: vec!["sub".into()],
            algorithm: Algorithm::Sha256,
            per_file: false,
            file_name: None,
        }))
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "sensitive_path");
    assert!(!root.path().join("sub.sha256").exists());
    let verified = engine
        .dispatch_mcp(Command::VerifyChecksumFile {
            panel: None,
            path: "list.sha256".into(),
        })
        .await
        .expect("verified");
    let Outcome::ChecksumsVerified { report, .. } = verified else {
        panic!("verified expected");
    };
    assert_eq!(report.entries[0].status, VerifyStatus::Ok);
    assert_eq!(report.entries[1].status, VerifyStatus::Unreadable);
    assert_eq!(report.entries[1].code.as_deref(), Some("sensitive_path"));
    assert_eq!(report.entries[1].actual, None);
}

/// The finished job of a `checksum.queue` command.
async fn queued(engine: &Engine, job: crate::ChecksumJob) -> rc_ops::JobSnapshot {
    let queued = engine.dispatch(Command::QueueChecksumJob(job)).await;
    let job = expect_job(queued.expect("queued"));
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        engine.ops().wait(job.id),
    )
    .await
    .expect("in time")
    .expect("known")
}

#[tokio::test]
async fn the_dialogs_jobs_write_check_split_and_combine_like_the_commands() {
    let (root, engine) = engine().await;
    let created = queued(
        &engine,
        crate::ChecksumJob::Create(crate::ChecksumFilesRequest {
            panel: None,
            dir: None,
            names: vec!["a.txt".into()],
            algorithm: Algorithm::Sha256,
            per_file: true,
            file_name: None,
        }),
    )
    .await;
    let Some(rc_ops::JobReport::ChecksumsCreated { report, .. }) = created.report else {
        panic!("a create report: {created:?}");
    };
    // The same digest `checksum.compute` (`files_checksum`) answers.
    assert_eq!(report.files[0].hex, SHA256_ABC);
    let verified = queued(
        &engine,
        crate::ChecksumJob::Verify {
            panel: None,
            path: "a.txt.sha256".into(),
        },
    )
    .await;
    let Some(rc_ops::JobReport::ChecksumsVerified { report, .. }) = verified.report else {
        panic!("a check's report: {verified:?}");
    };
    assert_eq!(report.entries[0].status, VerifyStatus::Ok);

    // A dialog's target is the other panel's folder, as Total Commander offers it.
    std::fs::create_dir(root.path().join("parts")).expect("parts");
    engine
        .dispatch(Command::Navigate {
            panel: Some(crate::PanelId::Right),
            path: root.path().join("parts").to_string_lossy().into_owned(),
        })
        .await
        .expect("the right panel shows the parts' folder");
    let split = queued(
        &engine,
        crate::ChecksumJob::Split(crate::SplitRequest {
            panel: None,
            path: "b.txt".into(),
            part_size: 2,
            target_dir: Some(".".into()),
        }),
    )
    .await;
    assert_eq!(split.state, rc_ops::JobState::Completed, "{split:?}");
    let combined = queued(
        &engine,
        crate::ChecksumJob::Combine(crate::CombineRequest {
            panel: None,
            path: "parts/b.001".into(),
            target_dir: Some(".".into()),
            target_name: Some("b again.txt".into()),
        }),
    )
    .await;
    assert_eq!(combined.state, rc_ops::JobState::Completed, "{combined:?}");
    assert_eq!(
        std::fs::read(root.path().join("parts/b again.txt")).expect("combined"),
        b"abc"
    );
}

#[tokio::test]
async fn a_dialog_is_announced_and_its_job_is_not_for_mcp() {
    let (_root, engine) = engine().await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ChecksumDialog {
            panel: Some(crate::PanelId::Right),
            dialog: crate::ChecksumDialog::Verify,
        })
        .await
        .expect("dialog");
    assert_eq!(
        events.try_recv().ok(),
        Some(crate::EngineEvent::ChecksumDialog {
            panel: crate::PanelId::Right,
            dialog: crate::ChecksumDialog::Verify,
        })
    );
    let job = crate::ChecksumJob::Verify {
        panel: None,
        path: "a.txt.sha256".into(),
    };
    let refused = crate::read_guard::as_mcp(async { engine.queue_checksum_job(job).await })
        .await
        .expect_err("refused");
    assert_eq!(refused.code(), "gui_only");
    assert!(engine.ops().jobs().is_empty(), "no job was queued");
}
