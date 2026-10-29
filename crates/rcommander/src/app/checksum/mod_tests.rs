//! The checksum dialogs through `apply`, against an engine over a temporary folder: the keys
//! open them, OK runs the job in the queue, its end fills the result list or the status bar.

use std::{fs, path::Path, time::Duration};

use rc_engine::{Command, CursorTarget, EngineEvent, JobSnapshot, JobState};
use tempfile::TempDir;

use super::*;
use crate::app::{Message, harness::Harness};

const SHA256_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// The left panel on `root` with `abc.txt` (and the cursor on it) and `big.bin`, the right one
/// on `out`.
async fn harness(root: &Path) -> Harness {
    fs::write(root.join("abc.txt"), b"abc").expect("write");
    fs::write(root.join("big.bin"), vec![7_u8; 3000]).expect("write");
    fs::create_dir(root.join("out")).expect("mkdir");
    let mut harness = Harness::open(root, &root.join("out")).await;
    cursor(&mut harness, PanelId::Left, "abc.txt").await;
    harness
}

async fn cursor(harness: &mut Harness, panel: PanelId, name: &str) {
    harness
        .app
        .engine()
        .dispatch(Command::SetCursor {
            panel: Some(panel),
            target: CursorTarget::Name(name.into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;
}

async fn send(harness: &mut Harness, message: ChecksumMsg) {
    harness.send(Message::Checksum(message)).await;
}

/// The newest job at its end, once the window has seen it end.
async fn finished(harness: &mut Harness) -> JobSnapshot {
    let engine = harness.app.engine().clone();
    let id = engine.ops().jobs().last().map(|job| job.id).expect("a job");
    let done = tokio::time::timeout(Duration::from_secs(20), engine.ops().wait(id))
        .await
        .expect("in time")
        .expect("known");
    harness.settle().await;
    done
}

fn results(harness: &Harness) -> &[Section] {
    match &harness.app.checksum {
        Some(ChecksumWindow::Results(sections)) => sections,
        other => panic!("the result list, got {other:?}"),
    }
}

/// Acceptance: Ctrl+Shift+F9 asks for the algorithm, writes the checksum file in a job, and the
/// result list shows the digest `files_checksum` answers for the same file.
#[tokio::test]
async fn ctrl_shift_f9_creates_a_checksum_file_and_lists_its_digest() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness.press("Ctrl+Shift+F9").await;
    let Some(ChecksumWindow::Create(create)) = &harness.app.checksum else {
        panic!("the create dialog");
    };
    assert_eq!(create.name, "abc.txt.sha256");
    send(
        &mut harness,
        ChecksumMsg::Algorithm(AlgorithmChoice(Algorithm::Md5)),
    )
    .await;
    send(
        &mut harness,
        ChecksumMsg::Algorithm(AlgorithmChoice(Algorithm::Sha256)),
    )
    .await;
    harness.press("Enter").await;
    assert_eq!(finished(&mut harness).await.state, JobState::Completed);
    let written = fs::read_to_string(root.path().join("abc.txt.sha256")).expect("written");
    assert_eq!(written, format!("{SHA256_ABC} *abc.txt\n"));
    let sections = results(&harness);
    assert_eq!(sections[0].rows[0].detail, SHA256_ABC);
    harness.press("Escape").await;
    assert!(harness.app.checksum.is_none());
}

/// The one checksum file's name is a file name, on every panel: a subfolder - with `\` as on
/// Windows or with `/` - keeps the dialog and starts no job (RC-020-05).
#[tokio::test]
async fn a_checksum_file_name_with_a_folder_keeps_the_dialog() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness.press("Ctrl+Shift+F9").await;
    for name in ["sub\\sums.sha256", "sub/sums.sha256", ".."] {
        send(&mut harness, ChecksumMsg::Name(name.into())).await;
        let Some(ChecksumWindow::Create(create)) = &harness.app.checksum else {
            panic!("the create dialog");
        };
        assert!(!create.name_ok(), "{name}");
        harness.press("Enter").await;
        assert!(
            matches!(harness.app.checksum, Some(ChecksumWindow::Create(_))),
            "{name}"
        );
        assert!(harness.app.engine().ops().jobs().is_empty(), "{name}");
    }
    // One checksum file for each file takes no name at all.
    send(&mut harness, ChecksumMsg::PerFile(true)).await;
    let Some(ChecksumWindow::Create(create)) = &harness.app.checksum else {
        panic!("the create dialog");
    };
    assert!(create.name_ok());
    send(&mut harness, ChecksumMsg::PerFile(false)).await;
    send(&mut harness, ChecksumMsg::Name(" sums.sha256 ".into())).await;
    harness.press("Enter").await;
    assert_eq!(finished(&mut harness).await.state, JobState::Completed);
    assert!(root.path().join("sums.sha256").is_file());
}

/// Ctrl+Shift+F10 checks the checksum file under the cursor at once; a changed file is caught.
#[tokio::test]
async fn ctrl_shift_f10_checks_and_lists_each_line() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    let text = format!("{SHA256_ABC} *abc.txt\n{SHA256_ABC} *big.bin\n");
    fs::write(root.path().join("set.sha256"), text).expect("write");
    harness
        .app
        .engine()
        .dispatch(Command::Refresh { panel: None })
        .await
        .expect("refresh");
    cursor(&mut harness, PanelId::Left, "set.sha256").await;
    harness.press("Ctrl+Shift+F10").await;
    assert_eq!(finished(&mut harness).await.state, JobState::Completed);
    let sections = results(&harness);
    let bad: Vec<bool> = sections[0].rows.iter().map(|row| row.bad).collect();
    // big.bin differs: it comes first, before abc.txt.
    assert_eq!(bad, vec![true, false]);
}

/// Ctrl+Shift+F11 splits into the other panel; a part size that will not do keeps the dialog.
/// Ctrl+Shift+F12 on a part puts the file back together in the other panel, under a new name.
#[tokio::test]
async fn split_into_the_other_panel_and_combine_back() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    cursor(&mut harness, PanelId::Left, "big.bin").await;
    harness.press("Ctrl+Shift+F11").await;
    let Some(ChecksumWindow::Split(split)) = &harness.app.checksum else {
        panic!("the split dialog");
    };
    assert_eq!(split.target, root.path().join("out").display().to_string());
    send(&mut harness, ChecksumMsg::PartSize("no size".into())).await;
    harness.press("Enter").await;
    assert!(matches!(
        harness.app.checksum,
        Some(ChecksumWindow::Split(_))
    ));
    assert!(harness.app.engine().ops().jobs().is_empty());
    send(&mut harness, ChecksumMsg::PartSize("1 KB".into())).await;
    harness.press("Enter").await;
    assert_eq!(finished(&mut harness).await.state, JobState::Completed);
    let out = root.path().join("out");
    for name in ["big.001", "big.002", "big.003", "big.crc"] {
        assert!(out.join(name).is_file(), "{name}");
    }
    let info = harness.app.info.clone().expect("the split's summary");
    assert!(info.contains("big.crc"), "{info}");

    cursor(&mut harness, PanelId::Right, "big.001").await;
    harness
        .send(Message::Engine(EngineEvent::ChecksumDialog {
            panel: PanelId::Right,
            dialog: ChecksumDialog::Combine,
        }))
        .await;
    let Some(ChecksumWindow::Combine(combine)) = &harness.app.checksum else {
        panic!("the combine dialog");
    };
    assert_eq!(combine.target, root.path().display().to_string());
    send(&mut harness, ChecksumMsg::Name("again.bin".into())).await;
    harness.press("Enter").await;
    assert_eq!(finished(&mut harness).await.state, JobState::Completed);
    assert_eq!(
        fs::read(root.path().join("again.bin")).expect("combined"),
        vec![7_u8; 3000]
    );
}

/// RC-080-38: on `..` without a selection the checksum keys do nothing - not for the first
/// entry the engine's cursor is on, as in Total Commander; with a selection as before.
#[tokio::test]
async fn on_the_parent_row_without_a_selection_no_checksum_dialog_opens() {
    let root = TempDir::new().expect("temp dir");
    let other = TempDir::new().expect("temp dir");
    fs::write(root.path().join("abc.txt"), b"abc").expect("write");
    let mut harness = Harness::open(root.path(), other.path()).await;
    harness.send(Message::Click(PanelId::Left, 0)).await;
    assert!(harness.app.pane(PanelId::Left).on_parent());
    for keys in ["Ctrl+Shift+F9", "Ctrl+Shift+F10", "Ctrl+Shift+F11"] {
        harness.press(keys).await;
        assert!(harness.app.checksum.is_none(), "{keys}");
    }
    assert!(
        harness.app.engine().ops().jobs().is_empty(),
        "nothing checked"
    );

    harness
        .app
        .engine()
        .dispatch(Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["abc.txt".into()],
        })
        .await
        .expect("selects");
    harness.settle().await;
    harness.press("Ctrl+Shift+F11").await;
    let Some(ChecksumWindow::Split(split)) = &harness.app.checksum else {
        panic!("the split dialog");
    };
    assert_eq!(split.file, "abc.txt");
    send(&mut harness, ChecksumMsg::Close).await;
    harness.press("Ctrl+Shift+F9").await;
    let Some(ChecksumWindow::Create(create)) = &harness.app.checksum else {
        panic!("the create dialog");
    };
    assert_eq!(create.names, ["abc.txt"]);
}
