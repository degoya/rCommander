//! F5 into an archive and F8 inside one (RC-110-05), through `apply`, against an engine over a
//! temporary folder.

use std::{fs, path::Path, time::Duration};

use rc_engine::{Command, CursorTarget, JobState, PackFormat, PackSettings, PanelId};
use tempfile::TempDir;

use super::super::ArchiveDialog;
use crate::app::harness::Harness;

/// The left panel on `root` (cursor on `notes.txt`), the right one inside `out/box.zip`, which
/// holds `docs/a.txt`.
async fn harness(root: &Path) -> Harness {
    fs::write(root.join("notes.txt"), b"notes").expect("write");
    fs::create_dir_all(root.join("docs")).expect("mkdir");
    fs::write(root.join("docs").join("a.txt"), b"alpha").expect("write");
    fs::create_dir(root.join("out")).expect("mkdir");
    let mut harness = Harness::open(root, &root.join("out")).await;
    let engine = harness.app.engine().clone();
    let packed = engine
        .dispatch(Command::ArchivePack {
            panel: Some(PanelId::Left),
            sources: vec!["docs".into()],
            target: Some("box.zip".into()),
            settings: PackSettings::default(),
            options: rc_engine::JobOptions::default(),
        })
        .await
        .expect("queued");
    let rc_engine::Outcome::Job(job) = packed else {
        panic!("a job");
    };
    wait(&harness, job.id).await;
    for (panel, path) in [
        (PanelId::Right, root.join("out").join("box.zip")),
        (PanelId::Left, root.to_path_buf()),
    ] {
        engine
            .dispatch(Command::Navigate {
                panel: Some(panel),
                path: path.to_string_lossy().into_owned(),
            })
            .await
            .expect("navigate");
    }
    cursor(&harness, PanelId::Left, "notes.txt").await;
    harness.settle().await;
    harness
}

async fn cursor(harness: &Harness, panel: PanelId, name: &str) {
    harness
        .app
        .engine()
        .dispatch(Command::SetCursor {
            panel: Some(panel),
            target: CursorTarget::Name(name.into()),
        })
        .await
        .expect("cursor");
}

async fn wait(harness: &Harness, id: rc_engine::JobId) -> rc_engine::JobSnapshot {
    tokio::time::timeout(Duration::from_secs(20), harness.app.engine().ops().wait(id))
        .await
        .expect("in time")
        .expect("known")
}

async fn last_job(harness: &Harness) -> rc_engine::JobSnapshot {
    let id = harness
        .app
        .engine()
        .ops()
        .jobs()
        .last()
        .map(|job| job.id)
        .expect("a job");
    wait(harness, id).await
}

fn lists(harness: &Harness, panel: PanelId, name: &str) -> bool {
    let state = harness.app.engine().panel(panel);
    state.entries.iter().any(|entry| entry.name == name)
}

/// Acceptance: F5 with an archive in the target panel opens the pack dialog adding to it - the
/// format the archive's, fixed - and OK adds; the panel then shows the new entry.
#[tokio::test]
async fn f5_into_an_archive_opens_the_add_dialog() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness.press("F5").await;
    let Some(ArchiveDialog::Pack(pack)) = &harness.app.archive else {
        panic!("the pack dialog, got {:?}", harness.app.archive);
    };
    assert!(pack.adding);
    assert_eq!(pack.settings.format, PackFormat::Zip);
    assert!(!pack.settings.delete_sources);
    assert!(pack.name.ends_with("box.zip"), "{}", pack.name);
    assert!(harness.app.file_ops.dialog.is_none(), "not the copy dialog");
    harness.press("Enter").await;
    assert!(harness.app.archive.is_none());
    let done = last_job(&harness).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    harness.settle().await;
    assert!(lists(&harness, PanelId::Right, "notes.txt"));
    assert!(
        root.path().join("notes.txt").is_file(),
        "F5 keeps the source"
    );
}

/// F6 into an archive is "move to archive": asked once more, the source goes afterwards.
#[tokio::test]
async fn f6_into_an_archive_moves_after_a_second_question() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness.press("F6").await;
    let Some(ArchiveDialog::Pack(pack)) = &harness.app.archive else {
        panic!("the pack dialog");
    };
    assert!(pack.adding && pack.settings.delete_sources);
    harness.press("Enter").await;
    assert!(harness.app.archive.is_some(), "asked once more");
    harness.press("Enter").await;
    let done = last_job(&harness).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(
        !root.path().join("notes.txt").exists(),
        "moved into the archive"
    );
}

/// Acceptance: F8 inside an archive asks with the count, and the entry goes for good.
#[tokio::test]
async fn f8_inside_an_archive_asks_with_the_count() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness
        .app
        .engine()
        .dispatch(Command::SwitchActivePanel {
            panel: Some(PanelId::Right),
        })
        .await
        .expect("the right panel");
    cursor(&harness, PanelId::Right, "docs").await;
    harness.settle().await;
    assert_eq!(harness.app.active, PanelId::Right);
    harness.press("F8").await;
    let Some(ArchiveDialog::Delete(prompt)) = &harness.app.archive else {
        panic!("the question, got {:?}", harness.app.archive);
    };
    assert_eq!(prompt.entries, ["docs"]);
    assert_eq!(prompt.archive, "box.zip");
    harness.press("Enter").await;
    let done = last_job(&harness).await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    harness.settle().await;
    assert!(!lists(&harness, PanelId::Right, "docs"));
}
