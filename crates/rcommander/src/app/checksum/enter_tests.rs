//! Enter and a double click on a checksum file through `apply`, against an engine over a
//! temporary folder (RC-020-10): the check of exactly that file, the result list with the bad
//! lines on top, and the setting that leaves the file to the system. No test lets a file open:
//! that would start a program.

use std::{fs, path::Path, time::Duration};

use rc_engine::{Command, CursorTarget, JobKind, JobSnapshot, JobState, PanelId, VfsPath};
use tempfile::TempDir;

use crate::app::{
    Effect, Message,
    checksum::{ChecksumMsg, ChecksumWindow, Section},
    context_menu::ContextMessage,
    harness::Harness,
};
use crate::i18n::t;

/// CRC32 of `abc`.
const CRC_ABC: &str = "352441C2";

/// The left panel on `root` with `abc.txt`, `big.bin`, the folder `dir.sfv` and `set.sfv`, which
/// lists `abc.txt` right, `big.bin` wrong and `gone.bin`, which is not there; the cursor on
/// `set.sfv`.
async fn harness(root: &Path) -> Harness {
    fs::write(root.join("abc.txt"), b"abc").expect("write");
    fs::write(root.join("big.bin"), vec![7_u8; 3000]).expect("write");
    fs::create_dir(root.join("dir.sfv")).expect("mkdir");
    let sfv = format!("; made by hand\nabc.txt {CRC_ABC}\nbig.bin 00000000\ngone.bin 12345678\n");
    fs::write(root.join("set.sfv"), sfv).expect("write");
    let mut harness = Harness::headless(root).await;
    cursor(&mut harness, "set.sfv").await;
    harness
}

async fn cursor(harness: &mut Harness, name: &str) {
    harness
        .app
        .engine()
        .dispatch(Command::SetCursor {
            panel: Some(PanelId::Left),
            target: CursorTarget::Name(name.into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;
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

/// The checked file's path and the result list's lines: name, status, bad.
fn checked(job: &JobSnapshot, harness: &Harness) -> (VfsPath, Vec<(String, bool)>) {
    let JobKind::ChecksumVerify { file } = &job.kind else {
        panic!("a check, got {:?}", job.kind);
    };
    let rows = results(harness)[0]
        .rows
        .iter()
        .map(|row| (row.name.clone(), row.bad))
        .collect();
    (file.clone(), rows)
}

/// The line the listing draws `name` on: its index past the `..` row.
fn row_of(harness: &Harness, name: &str) -> usize {
    let pane = harness.app.pane(PanelId::Left);
    let index = pane
        .entries()
        .iter()
        .position(|entry| entry.name == name)
        .expect("listed");
    index + usize::from(pane.has_parent_row())
}

fn enter() -> Command {
    Command::OpenFile {
        panel: None,
        path: None,
    }
}

/// Acceptance: Enter on a `.sfv` checks exactly that file - not the selected `abc.txt` - in one
/// job, and the list shows the changed and the missing file on top, each with its status.
#[tokio::test]
async fn enter_checks_the_checksum_file_under_the_cursor_with_the_bad_lines_on_top() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    harness
        .app
        .engine()
        .dispatch(Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["abc.txt".into()],
        })
        .await
        .expect("select");
    harness.settle().await;
    harness.press("Enter").await;
    let job = finished(&mut harness).await;
    assert_eq!(job.state, JobState::Completed);
    assert_eq!(harness.app.engine().ops().jobs().len(), 1);
    let (file, rows) = checked(&job, &harness);
    assert_eq!(file.path, root.path().join("set.sfv"));
    let bad: Vec<bool> = rows.iter().map(|(_, bad)| *bad).collect();
    assert_eq!(bad, vec![true, true, false]);
    let mut on_top: Vec<&str> = rows[..2].iter().map(|(name, _)| name.as_str()).collect();
    on_top.sort_unstable();
    assert_eq!(on_top, vec!["big.bin", "gone.bin"]);
    let section = &results(&harness)[0];
    assert_ne!(section.rows[0].status, section.rows[1].status);
    assert_ne!(section.rows[0].status, section.rows[2].status);
}

/// A double click on a checksum file moves the cursor there and checks it.
#[tokio::test]
async fn a_double_click_on_a_checksum_file_checks_it() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    cursor(&mut harness, "abc.txt").await;
    let row = row_of(&harness, "set.sfv");
    harness.send(Message::Open(PanelId::Left, row)).await;
    let job = finished(&mut harness).await;
    assert_eq!(job.state, JobState::Completed);
    let (file, rows) = checked(&job, &harness);
    assert_eq!(file.path, root.path().join("set.sfv"));
    assert_eq!(rows.len(), 3);
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!(
        pane.state.cursor_entry().map(|entry| entry.name.as_str()),
        Some("set.sfv")
    );
}

/// Another file and a folder named like a checksum file are left to the engine's open, and so
/// is every other command.
#[tokio::test]
async fn enter_on_another_file_or_a_folder_stays_the_open() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    assert!(
        harness
            .app
            .checksum_enter(PanelId::Left, &enter())
            .is_some()
    );
    let view = Command::Refresh { panel: None };
    assert!(harness.app.checksum_enter(PanelId::Left, &view).is_none());
    for name in ["abc.txt", "dir.sfv"] {
        cursor(&mut harness, name).await;
        assert!(
            harness
                .app
                .checksum_enter(PanelId::Left, &enter())
                .is_none(),
            "{name}"
        );
        let row = row_of(&harness, name);
        let effects = harness.app.double_click(PanelId::Left, row);
        assert!(opens(&effects), "{name}");
    }
    assert!(harness.app.engine().ops().jobs().is_empty());
}

/// `checksum.enter_verifies` off: Enter and a double click open a checksum file as before; on
/// again, they check it.
#[tokio::test]
async fn the_setting_off_leaves_a_checksum_file_to_the_open() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    let row = row_of(&harness, "set.sfv");
    harness
        .send(Message::Checksum(ChecksumMsg::EnterVerifies(false)))
        .await;
    assert!(
        harness
            .app
            .checksum_enter(PanelId::Left, &enter())
            .is_none()
    );
    assert!(opens(&harness.app.double_click(PanelId::Left, row)));
    harness
        .send(Message::Checksum(ChecksumMsg::EnterVerifies(true)))
        .await;
    let effects = harness
        .app
        .checksum_enter(PanelId::Left, &enter())
        .expect("a check");
    assert!(matches!(effects.as_slice(), [Effect::Ops(_)]));
    let effects = harness.app.double_click(PanelId::Left, row);
    assert!(!opens(&effects));
    assert!(matches!(
        effects.as_slice(),
        [Effect::Dispatch(..), Effect::Ops(_)]
    ));
    assert!(harness.app.engine().ops().jobs().is_empty());
}

/// Whether `effects` are a double click's open: one job that ends in `file.open`.
fn opens(effects: &[Effect]) -> bool {
    matches!(
        effects,
        [Effect::Dispatch(_, commands)]
            if matches!(commands.last(), Some(Command::OpenFile { path: None, .. }))
    )
}

/// The context menu of a checksum file offers "Verify checksums", which checks it; another file
/// and a folder named like one get no such entry.
#[tokio::test]
async fn the_context_menu_of_a_checksum_file_verifies_it() {
    let root = TempDir::new().expect("temp dir");
    let mut harness = harness(root.path()).await;
    let verify: rc_engine::Chord = "Ctrl+Shift+F10".parse().expect("a chord");
    for name in ["abc.txt", "dir.sfv"] {
        let row = row_of(&harness, name);
        harness
            .send(Message::ContextMenu(ContextMessage::Row(
                PanelId::Left,
                row,
            )))
            .await;
        let offered = harness
            .app
            .context_menu
            .as_ref()
            .is_some_and(|open| open.items.iter().any(|item| item.key() == Some(&verify)));
        assert!(!offered, "{name}");
        harness
            .send(Message::ContextMenu(ContextMessage::Close))
            .await;
    }
    let row = row_of(&harness, "set.sfv");
    harness
        .send(Message::ContextMenu(ContextMessage::Row(
            PanelId::Left,
            row,
        )))
        .await;
    let open = harness.app.context_menu.as_ref().expect("an open menu");
    let item = open
        .items
        .iter()
        .find(|item| item.key() == Some(&verify))
        .expect("Verify checksums");
    assert_eq!(item.label, t!("context_menu.verify_checksums"));
    let choice = item.choice.clone();
    harness
        .send(Message::ContextMenu(ContextMessage::Pick(choice)))
        .await;
    let job = finished(&mut harness).await;
    let (file, _) = checked(&job, &harness);
    assert_eq!(file.path, root.path().join("set.sfv"));
}
