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

/// The archive at `archive` as the archive backend reads it.
async fn archive_fs(archive: &std::path::Path) -> rc_vfs::ArchiveFs {
    let outer: std::sync::Arc<dyn rc_vfs::Vfs> = std::sync::Arc::new(rc_vfs::LocalFs::new());
    rc_vfs::ArchiveFs::open(outer, archive, rc_vfs::ArchiveSettings::default())
        .await
        .expect("open")
        .expect("an archive")
}

/// RC-110-05: F5 into an archive adds to the folder named inside it, F8 inside the archive the
/// panel shows removes an entry with everything below it, for good.
#[tokio::test]
async fn files_go_into_an_archive_and_entries_out_of_it() {
    use rc_vfs::Vfs as _;
    let (root, engine) = engine().await;
    let packed = run(
        &engine,
        Command::ArchivePack {
            panel: Some(PanelId::Left),
            sources: vec!["docs".into()],
            target: Some("box.zip".into()),
            settings: PackSettings::default(),
            options: JobOptions::default(),
        },
    )
    .await;
    assert_eq!(packed.state, JobState::Completed, "{packed:?}");
    let zip = root.path().join("out").join("box.zip");
    let added = run(
        &engine,
        Command::ArchiveAdd {
            panel: Some(PanelId::Left),
            sources: vec!["notes.txt".into()],
            target: Some(zip.join("sub").to_string_lossy().into_owned()),
            settings: rc_ops::UpdateSettings::default(),
            options: JobOptions::default(),
        },
    )
    .await;
    assert_eq!(added.state, JobState::Completed, "{added:?}");
    let fs = archive_fs(&zip).await;
    assert!(
        fs.stat(std::path::Path::new("/sub/notes.txt"))
            .await
            .is_ok()
    );
    assert!(fs.stat(std::path::Path::new("/docs/a.txt")).await.is_ok());

    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: zip.to_string_lossy().into_owned(),
        })
        .await
        .expect("into the archive");
    let deleted = run(
        &engine,
        Command::ArchiveDelete {
            panel: Some(PanelId::Right),
            entries: vec!["docs".into()],
            options: JobOptions::default(),
        },
    )
    .await;
    assert_eq!(deleted.state, JobState::Completed, "{deleted:?}");
    let fs = archive_fs(&zip).await;
    assert!(fs.stat(std::path::Path::new("/docs/a.txt")).await.is_err());
    assert!(
        fs.stat(std::path::Path::new("/sub/notes.txt"))
            .await
            .is_ok()
    );
}

/// Adding that deletes the sources or overwrites is gated as destructive; deleting always is.
/// A compressed tar is refused before a job is queued.
#[tokio::test]
async fn changing_an_archive_is_classed_and_a_tar_gz_refused() {
    use crate::registry::CommandClass;
    let add = |delete_sources: bool, on_conflict| Command::ArchiveAdd {
        panel: None,
        sources: Vec::new(),
        target: None,
        settings: rc_ops::UpdateSettings {
            delete_sources,
            ..rc_ops::UpdateSettings::default()
        },
        options: JobOptions {
            on_conflict,
            ..JobOptions::default()
        },
    };
    assert_eq!(add(false, None).effective_class(), CommandClass::Write);
    assert_eq!(add(true, None).effective_class(), CommandClass::Destructive);
    let overwrite = Some(rc_ops::ConflictChoice::Overwrite);
    assert_eq!(
        add(false, overwrite).effective_class(),
        CommandClass::Destructive
    );
    let delete = Command::ArchiveDelete {
        panel: None,
        entries: Vec::new(),
        options: JobOptions::default(),
    };
    assert_eq!(delete.effective_class(), CommandClass::Destructive);

    let (root, engine) = engine().await;
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives/tree.tar.gz");
    let tgz = root.path().join("out").join("tree.tar.gz");
    fs::copy(fixture, &tgz).expect("copy");
    let refused = engine
        .dispatch(Command::ArchiveAdd {
            panel: Some(PanelId::Left),
            sources: vec!["notes.txt".into()],
            target: Some(tgz.to_string_lossy().into_owned()),
            settings: rc_ops::UpdateSettings::default(),
            options: JobOptions::default(),
        })
        .await;
    assert_eq!(
        refused.map(|_| ()).map_err(|error| error.code()),
        Err("archive_not_modifiable")
    );
    let message = crate::EngineError::ArchiveReadOnly.to_string();
    assert!(
        message.contains("F5") && message.contains("archive.delete"),
        "{message}"
    );
}
