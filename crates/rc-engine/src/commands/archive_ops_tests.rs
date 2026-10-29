use std::{fs, time::Duration};

use rc_ops::{ErrorPolicy, JobOptions, JobState, PackSettings, Question};

use crate::test_support;
use crate::test_support::expect_job;
use crate::{Command, Engine, EngineEvent, JobChange, Outcome, PanelId, SecretParam};

async fn engine() -> (tempfile::TempDir, Engine) {
    let root = test_support::tree(&[
        ("docs/a.txt", "alpha"),
        ("notes.txt", "notes"),
        ("out/", ""),
    ]);
    let engine = test_support::open(root.path(), &root.path().join("out")).await;
    (root, engine)
}

async fn run(engine: &Engine, command: Command) -> rc_ops::JobSnapshot {
    let job = expect_job(engine.dispatch(command).await.expect("queued"));
    tokio::time::timeout(Duration::from_secs(20), engine.ops().wait(job.id))
        .await
        .expect("in time")
        .expect("known")
}

/// Alt+F5 without a name: the archive is named after the entry, in the other panel; Alt+F9
/// unpacks it there.
#[tokio::test]
async fn pack_names_the_archive_after_the_source_and_extract_unpacks_it() {
    let (root, engine) = engine().await;
    let packed = run(
        &engine,
        Command::ArchivePack {
            panel: Some(PanelId::Left),
            sources: vec!["docs".into(), "notes.txt".into()],
            target: None,
            settings: PackSettings::default(),
            options: JobOptions::default(),
        },
    )
    .await;
    assert_eq!(packed.state, JobState::Completed, "{packed:?}");
    let dir_name = root
        .path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .expect("name");
    let archive = root.path().join("out").join(format!("{dir_name}.zip"));
    assert!(
        archive.is_file(),
        "several sources: named after their folder"
    );

    let single = run(
        &engine,
        Command::ArchivePack {
            panel: Some(PanelId::Left),
            sources: vec!["notes.txt".into()],
            target: None,
            settings: PackSettings::default(),
            options: JobOptions::default(),
        },
    )
    .await;
    assert_eq!(single.state, JobState::Completed, "{single:?}");
    assert!(root.path().join("out").join("notes.zip").is_file());

    let unpacked = run(
        &engine,
        Command::ArchiveExtract {
            panel: Some(PanelId::Right),
            archives: vec![format!("{dir_name}.zip")],
            target: Some(root.path().join("unpacked").to_string_lossy().into_owned()),
            settings: rc_ops::ExtractSettings::default(),
            options: JobOptions::default(),
        },
    )
    .await;
    assert_eq!(unpacked.state, JobState::Completed, "{unpacked:?}");
    let a = root.path().join("unpacked").join("docs").join("a.txt");
    assert_eq!(fs::read(a).expect("unpacked"), b"alpha");
}

/// The password reaches the job through `archive.password`, which MCP may not run.
#[tokio::test]
async fn the_password_comes_only_from_the_window() {
    let (root, engine) = engine().await;
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives/encrypted.7z");
    let refused = engine
        .dispatch_mcp(Command::ArchivePassword {
            job: 1,
            password: Some(SecretParam::from("secret")),
        })
        .await;
    assert_eq!(refused.map(|_| ()).map_err(|e| e.code()), Err("gui_only"));

    let mut events = engine.subscribe();
    let queued = engine
        .dispatch(Command::ArchiveExtract {
            panel: None,
            archives: vec![fixture.to_string_lossy().into_owned()],
            target: Some(root.path().join("secret").to_string_lossy().into_owned()),
            settings: rc_ops::ExtractSettings::default(),
            options: JobOptions {
                on_error: ErrorPolicy::Ask,
                ..JobOptions::default()
            },
        })
        .await
        .expect("queued");
    let Outcome::Job(job) = queued else {
        panic!("a job");
    };
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("in time")
            .expect("event");
        if let EngineEvent::Job {
            change:
                JobChange::Question {
                    question: Question::Password { .. },
                },
            ..
        } = event
        {
            break;
        }
    }
    engine
        .dispatch(Command::ArchivePassword {
            job: job.id,
            password: Some(SecretParam::from("secret")),
        })
        .await
        .expect("answered");
    let done = tokio::time::timeout(Duration::from_secs(10), engine.ops().wait(job.id))
        .await
        .expect("in time")
        .expect("known");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let secret = root.path().join("secret").join("secret.txt");
    assert_eq!(fs::read(secret).expect("unpacked"), b"7z secret\n");
}
