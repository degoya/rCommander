#[cfg(not(windows))]
use std::path::PathBuf;

use rc_engine::{
    ArchiveEntryInfo, Failure, JobKind, JobOptions, JobSnapshot, JobState, Progress, UnsafeName,
    VfsPath,
};
use rc_vfs::ArchiveFormat;

use super::{breadcrumb_of, marks, packed, test_summary};

fn inside(path: &str) -> VfsPath {
    VfsPath::archive(VfsPath::local("/data/backup.zip"), ArchiveFormat::Zip, path)
}

fn info(encrypted: bool, unsafe_name: Option<UnsafeName>) -> ArchiveEntryInfo {
    ArchiveEntryInfo {
        stored_name: "../evil.txt".into(),
        unsafe_name,
        encrypted,
        packed_size: Some(2048),
        implied: false,
    }
}

#[cfg(not(windows))]
#[test]
fn the_path_bar_leads_out_of_the_archive_by_steps_up_and_inside_from_its_root() {
    let segments = breadcrumb_of(&inside("/docs/sub"));
    let targets: Vec<(String, PathBuf)> = segments;
    assert_eq!(
        targets,
        [
            ("/".to_owned(), PathBuf::from("../../../..")),
            ("data".to_owned(), PathBuf::from("../../..")),
            ("[backup.zip]".to_owned(), PathBuf::from("/")),
            ("docs".to_owned(), PathBuf::from("/docs")),
            ("sub".to_owned(), PathBuf::from("/docs/sub")),
        ]
    );
    assert_eq!(
        breadcrumb_of(&VfsPath::local("/data")),
        [
            ("/".to_owned(), PathBuf::from("/")),
            ("data".to_owned(), PathBuf::from("/data")),
        ]
    );
}

#[test]
fn encrypted_and_unsafe_entries_are_marked() {
    assert_eq!(marks(None), (String::new(), false));
    assert_eq!(marks(Some(&info(false, None))), (String::new(), false));
    let (mark, warn) = marks(Some(&info(true, Some(UnsafeName::ParentDir))));
    assert!(warn);
    assert!(mark.contains(crate::i18n::t!("archive.mark.encrypted")));
    assert!(mark.contains(crate::i18n::t!("archive.mark.unsafe")));
    assert!(!packed(Some(&info(false, None))).is_empty());
    assert!(packed(None).is_empty());
}

#[test]
fn an_archive_test_reports_passed_damaged_and_encrypted_entries() {
    let failure = |name: &str, code: &str| Failure {
        path: inside(&format!("/{name}")),
        code: code.into(),
        message: String::new(),
        target: None,
    };
    let job = JobSnapshot {
        id: 1,
        kind: JobKind::TestArchive {
            archive: inside("/"),
        },
        options: JobOptions::default(),
        state: JobState::Completed,
        progress: Progress {
            files_total: 10,
            files_done: 10,
            ..Progress::default()
        },
        question: None,
        skipped: 1,
        failures: vec![
            failure("hello.txt", "archive_damaged"),
            failure("secret.txt", "password_required"),
        ],
        report: None,
        resumable: false,
        batch: None,
    };
    let summary = test_summary(&job).expect("a test");
    assert!(summary.contains("backup.zip"), "{summary}");
    assert!(summary.contains("hello.txt"), "{summary}");
    assert!(!summary.contains("secret.txt"), "{summary}");
    assert!(summary.contains('7'), "seven passed: {summary}");
    let copy = JobSnapshot {
        kind: JobKind::Mkdir {
            path: VfsPath::local("/x"),
        },
        ..job
    };
    assert_eq!(test_summary(&copy), None);
}

/// A folder holding `tree.zip`, and the app over an engine whose panels show it, the cursor on
/// the archive.
async fn beside_an_archive() -> (tempfile::TempDir, crate::app::harness::Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives/tree.zip");
    std::fs::copy(fixture, root.path().join("tree.zip")).expect("copy the fixture");
    let mut harness = crate::app::harness::Harness::headless(root.path()).await;
    // From the `..` row onto the archive, the only entry.
    harness.press("ArrowDown").await;
    assert!(!harness.app.pane(rc_engine::PanelId::Left).on_parent());
    (root, harness)
}

#[tokio::test]
async fn enter_opens_the_archive_and_backspace_returns_onto_it() {
    let (root, mut harness) = beside_an_archive().await;
    harness.press("Enter").await;
    let pane = harness.app.pane(rc_engine::PanelId::Left);
    assert!(
        rc_engine::archive_file(&pane.state.path).is_some(),
        "inside"
    );
    assert!(pane.has_parent_row(), ".. leads out of the archive");
    assert!(pane.on_parent());
    assert!(pane.state.archive.is_some(), "the archive's details");
    assert_eq!(
        breadcrumb_of(&pane.state.path)
            .last()
            .map(|(label, _)| label.clone()),
        Some("[tree.zip]".to_owned())
    );

    harness.press("Backspace").await;
    let pane = harness.app.pane(rc_engine::PanelId::Left);
    assert_eq!(pane.state.path, VfsPath::local(root.path()));
    assert_eq!(
        pane.state.cursor_entry().map(|entry| entry.name.as_str()),
        Some("tree.zip")
    );
    assert!(!pane.on_parent());
}

#[tokio::test]
async fn alt_shift_f9_tests_the_archive_and_reports_in_the_status_bar() {
    let (_root, mut harness) = beside_an_archive().await;
    harness.press("Ctrl+PageDown").await;
    harness.press("Alt+Shift+F9").await;
    let jobs = harness.app.engine().ops().jobs();
    let job = jobs.last().expect("a test job");
    harness
        .app
        .engine()
        .ops()
        .wait(job.id)
        .await
        .expect("finishes");
    harness.settle().await;
    let info = harness.app.info.clone().unwrap_or_default();
    assert!(info.contains("tree.zip"), "{info}");
    assert!(harness.app.pane(rc_engine::PanelId::Left).error.is_none());
}
