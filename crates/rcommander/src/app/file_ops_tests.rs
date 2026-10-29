//! The file operations through `apply`, without a window (RC-010-07): a key opens its dialog, OK
//! starts the job on a real engine, its question is answered in the dialog, and when it is over
//! the panels show the result.

use std::{fs, path::Path, time::Duration};

use rc_engine::{Command, ConflictChoice, FileDialog, JobId, PanelId};
use tempfile::TempDir;

use super::*;
use crate::app::harness::Harness;

/// `root/` with `a.txt`, `b.txt` and `out/`; the left panel on `root`, the right one on `out`.
async fn setup() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("out")).expect("mkdir");
    fs::write(root.path().join("a.txt"), b"new").expect("write");
    fs::write(root.path().join("b.txt"), b"b").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    run(
        &mut harness,
        Command::Navigate {
            panel: Some(PanelId::Right),
            path: "out".into(),
        },
    )
    .await;
    (root, harness)
}

/// Runs `command` on the engine and lets the app see what it announced.
async fn run(harness: &mut Harness, command: Command) {
    harness
        .app
        .engine()
        .dispatch(command)
        .await
        .expect("dispatched");
    harness.settle().await;
}

async fn cursor_to(harness: &mut Harness, name: &str) {
    run(
        harness,
        Command::SetCursor {
            panel: Some(PanelId::Left),
            target: rc_engine::CursorTarget::Name(name.into()),
        },
    )
    .await;
}

fn send(message: FileOpsMsg) -> Message {
    Message::FileOps(message)
}

/// The newest job of the queue.
fn last_job(harness: &Harness) -> JobId {
    harness
        .app
        .engine()
        .ops()
        .jobs()
        .last()
        .map(|job| job.id)
        .expect("a job")
}

/// Waits until job `id` is over, then lets the app see its end.
async fn finish(harness: &mut Harness, id: JobId) {
    tokio::time::timeout(Duration::from_secs(10), harness.app.engine().ops().wait(id))
        .await
        .expect("in time")
        .expect("known");
    harness.settle().await;
}

/// Waits until job `id` asks something, then lets the app see the question.
async fn asked(harness: &mut Harness, id: JobId) {
    for _ in 0..500 {
        let job = harness.app.engine().ops().job(id).expect("known");
        if job.question.is_some() {
            harness.settle().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} asked nothing");
}

fn names(harness: &Harness, panel: PanelId) -> Vec<String> {
    let pane = harness.app.pane(panel);
    pane.entries()
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

#[tokio::test]
async fn f5_opens_the_copy_dialog_and_escape_closes_it() {
    let (root, mut harness) = setup().await;
    cursor_to(&mut harness, "a.txt").await;
    harness.press("F5").await;
    let dialog = harness
        .app
        .file_ops
        .dialog
        .clone()
        .expect("the copy dialog");
    assert_eq!(dialog.kind, FileDialog::Copy);
    assert!(
        dialog
            .text
            .starts_with(&root.path().join("out").display().to_string())
    );
    let before = harness.app.engine().panel(PanelId::Left).cursor;
    harness.press("ArrowDown").await;
    assert_eq!(
        harness.app.engine().panel(PanelId::Left).cursor,
        before,
        "the dialog has the keys"
    );
    harness.press("Escape").await;
    assert!(harness.app.file_ops.dialog.is_none());
}

#[tokio::test]
async fn ok_copies_into_the_other_panel_refreshes_both_and_deselects() {
    let (root, mut harness) = setup().await;
    run(
        &mut harness,
        Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["a.txt".into()],
        },
    )
    .await;
    harness.press("F5").await;
    harness
        .send(send(FileOpsMsg::Start { background: false }))
        .await;
    let job = last_job(&harness);
    finish(&mut harness, job).await;
    assert_eq!(
        fs::read(root.path().join("out").join("a.txt")).expect("copied"),
        b"new"
    );
    assert_eq!(names(&harness, PanelId::Right), ["a.txt"]);
    assert!(harness.app.pane(PanelId::Left).state.selection.is_empty());
    assert!(harness.app.file_ops.jobs.is_empty());
    assert!(!harness.app.file_ops.modal());
}

/// F6 deselects its sources when it starts, not when it is over (owner, 2026-10-19): here the
/// move still waits on a conflict.
#[tokio::test]
async fn f6_deselects_its_sources_when_it_starts() {
    let (root, mut harness) = setup().await;
    fs::write(root.path().join("out").join("a.txt"), b"old").expect("write");
    run(
        &mut harness,
        Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["a.txt".into()],
        },
    )
    .await;
    harness.press("F6").await;
    harness
        .send(send(FileOpsMsg::Start { background: false }))
        .await;
    let job = last_job(&harness);
    asked(&mut harness, job).await;
    assert!(harness.app.pane(PanelId::Left).state.selection.is_empty());
    harness.send(send(FileOpsMsg::Abort(job))).await;
    finish(&mut harness, job).await;
}

/// The engine puts the cursor on the first entry (`panel.set_cursor`, as over MCP): the window's
/// cursor leaves the `..` row with it, so F5 copies that entry.
#[tokio::test]
async fn f5_copies_the_first_entry_the_engine_put_the_cursor_on() {
    let root = tempfile::tempdir().expect("temp dir");
    let (left, right) = (root.path().join("left"), root.path().join("right"));
    fs::create_dir(&left).expect("mkdir");
    fs::create_dir(&right).expect("mkdir");
    fs::write(left.join("a.txt"), b"first").expect("write");
    fs::write(left.join("b.txt"), b"b").expect("write");
    let mut harness = Harness::open(&left, &right).await;
    assert!(harness.app.pane(PanelId::Left).on_parent());
    cursor_to(&mut harness, "a.txt").await;
    assert_eq!(harness.app.pane(PanelId::Left).state.cursor, 0);
    assert!(!harness.app.pane(PanelId::Left).on_parent());
    harness.press("F5").await;
    harness
        .send(send(FileOpsMsg::Start { background: false }))
        .await;
    let job = last_job(&harness);
    finish(&mut harness, job).await;
    assert_eq!(fs::read(right.join("a.txt")).expect("copied"), b"first");
    assert!(!right.join("b.txt").exists());
}

/// The acceptance check: a conflict shows its dialog, and its answer works as in RC-010-06.
#[tokio::test]
async fn an_existing_target_asks_and_the_answer_overwrites() {
    let (root, mut harness) = setup().await;
    fs::write(root.path().join("out").join("a.txt"), b"old").expect("write");
    cursor_to(&mut harness, "a.txt").await;
    harness.press("F5").await;
    harness
        .send(send(FileOpsMsg::Start { background: false }))
        .await;
    let job = last_job(&harness);
    asked(&mut harness, job).await;
    let asking = harness.app.file_ops.jobs.asking().expect("a question");
    assert_eq!(asking.rename_to, "a (2).txt");
    harness
        .send(send(FileOpsMsg::Conflict {
            job,
            choice: ConflictChoice::Overwrite,
            for_all: false,
        }))
        .await;
    finish(&mut harness, job).await;
    assert_eq!(
        fs::read(root.path().join("out").join("a.txt")).expect("read"),
        b"new"
    );
}

#[tokio::test]
async fn escape_answers_the_conflict_question_with_cancel() {
    let (root, mut harness) = setup().await;
    fs::write(root.path().join("out").join("a.txt"), b"old").expect("write");
    cursor_to(&mut harness, "a.txt").await;
    harness.press("F5").await;
    harness
        .send(send(FileOpsMsg::Start { background: false }))
        .await;
    let job = last_job(&harness);
    asked(&mut harness, job).await;
    // Without an answer the job would wait and `finish` would run out of time.
    harness.press("Escape").await;
    finish(&mut harness, job).await;
    assert!(harness.app.file_ops.jobs.asking().is_none());
    assert_eq!(
        fs::read(root.path().join("out").join("a.txt")).expect("read"),
        b"old"
    );
}

#[tokio::test]
async fn rename_to_a_bad_name_keeps_the_question_with_its_reason() {
    let (root, mut harness) = setup().await;
    fs::write(root.path().join("out").join("a.txt"), b"old").expect("write");
    cursor_to(&mut harness, "a.txt").await;
    harness.press("F5").await;
    harness
        .send(send(FileOpsMsg::Start { background: false }))
        .await;
    let job = last_job(&harness);
    asked(&mut harness, job).await;
    harness
        .send(send(FileOpsMsg::ProposalInput(job, "bad/name".into())))
        .await;
    harness.send(send(FileOpsMsg::RenameTo(job))).await;
    let asking = harness.app.file_ops.jobs.asking().expect("still asking");
    assert_eq!(asking.note, Some("invalid_name"));
    harness
        .send(send(FileOpsMsg::ProposalInput(job, "a-copy.txt".into())))
        .await;
    harness.send(send(FileOpsMsg::RenameTo(job))).await;
    finish(&mut harness, job).await;
    assert_eq!(
        fs::read(root.path().join("out").join("a-copy.txt")).expect("read"),
        b"new"
    );
    assert_eq!(
        fs::read(root.path().join("out").join("a.txt")).expect("read"),
        b"old"
    );
}

/// The acceptance check: a job in the background leaves the window to the keys, and a second
/// job starts beside it.
#[tokio::test]
async fn a_background_job_leaves_the_window_free_for_a_second() {
    let (root, mut harness) = setup().await;
    cursor_to(&mut harness, "a.txt").await;
    harness.press("F5").await;
    harness
        .send(send(FileOpsMsg::Start { background: true }))
        .await;
    let first = last_job(&harness);
    assert!(!harness.app.file_ops.modal(), "no progress window");
    cursor_to(&mut harness, "b.txt").await;
    harness.press("F6").await;
    assert!(harness.app.file_ops.dialog.is_some(), "the keys still work");
    harness
        .send(send(FileOpsMsg::Start { background: true }))
        .await;
    let second = last_job(&harness);
    assert_ne!(first, second);
    finish(&mut harness, first).await;
    finish(&mut harness, second).await;
    assert!(root.path().join("out").join("a.txt").exists());
    assert!(root.path().join("out").join("b.txt").exists());
    assert!(!root.path().join("b.txt").exists(), "moved");
}

#[tokio::test]
async fn f7_makes_nested_folders_with_the_cursor_on_the_first() {
    let (root, mut harness) = setup().await;
    harness.press("F7").await;
    harness
        .send(send(FileOpsMsg::Input(format!(
            "new{}deeper",
            std::path::MAIN_SEPARATOR
        ))))
        .await;
    harness.press("Enter").await;
    let job = last_job(&harness);
    finish(&mut harness, job).await;
    harness.settle().await;
    assert!(root.path().join("new").join("deeper").is_dir());
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!(pane.entries()[pane.cursor()].name, "new");
}

/// `mkdir.prefill_cursor_name` off, F7 starts with an empty name instead of the cursor's.
#[tokio::test]
async fn f7_offers_the_cursor_name_until_switched_off() {
    let (_root, mut harness) = setup().await;
    cursor_to(&mut harness, "out").await;
    harness.press("F7").await;
    let dialog = harness.app.file_ops.dialog.clone().expect("the dialog");
    assert_eq!(dialog.text, "out");
    harness.press("Escape").await;
    let off = FileOpsMsg::Settings(strip::Stored {
        prefill: false,
        ..strip::Stored::default()
    });
    harness.send(send(off)).await;
    harness.press("F7").await;
    let dialog = harness.app.file_ops.dialog.clone().expect("the dialog");
    assert_eq!(dialog.text, "");
}

#[tokio::test]
async fn shift_f6_renames_in_the_list() {
    let (root, mut harness) = setup().await;
    cursor_to(&mut harness, "b.txt").await;
    harness.press("Shift+F6").await;
    let rename = harness.app.file_ops.rename.clone().expect("the editor");
    assert_eq!((rename.original.as_str(), rename.selected), ("b.txt", 1));
    harness
        .send(send(FileOpsMsg::RenameInput("c.txt".into())))
        .await;
    harness.send(send(FileOpsMsg::RenameSubmit)).await;
    let job = last_job(&harness);
    finish(&mut harness, job).await;
    harness.settle().await;
    assert!(root.path().join("c.txt").exists());
    assert!(!root.path().join("b.txt").exists());
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!(pane.entries()[pane.cursor()].name, "c.txt");
}

/// RC-080-17: where the folder has no recycle bin, F8 asks as Shift+F8 does once
/// `delete.permanent_without_trash` is on, and deletes for good after the question; off, F8
/// keeps its own question.
#[tokio::test]
async fn f8_without_a_bin_deletes_for_good_after_the_question_when_switched_on() {
    let (root, mut harness) = setup().await;
    let engine = harness.app.engine().clone();
    assert!(engine.attach_db(rc_db::Db::open_in_memory().await.expect("db")));
    // The local folder without a recycle bin, as a share or a server is.
    engine.ops().register(
        rc_vfs::BackendId::Local,
        rc_ops::Backend::new(std::sync::Arc::new(rc_vfs::LocalFs::new())),
    );
    cursor_to(&mut harness, "b.txt").await;
    harness.press("F8").await;
    let dialog = harness.app.file_ops.dialog.clone().expect("the question");
    assert_eq!(dialog.kind, FileDialog::Delete, "off: as before");
    harness.press("Escape").await;

    let on = Command::SetSetting {
        key: rc_engine::settings::PERMANENT_WITHOUT_TRASH_KEY.into(),
        value: serde_json::json!(true),
    };
    run(&mut harness, on).await;
    harness.press("F8").await;
    let dialog = harness.app.file_ops.dialog.clone().expect("the question");
    assert_eq!(dialog.kind, FileDialog::DeletePermanent, "says for good");
    assert!(root.path().join("b.txt").exists(), "nothing before OK");
    harness.press("Enter").await;
    let job = last_job(&harness);
    finish(&mut harness, job).await;
    assert!(!root.path().join("b.txt").exists());
}

#[tokio::test]
async fn shift_f8_deletes_for_good_after_its_question() {
    let (root, mut harness) = setup().await;
    cursor_to(&mut harness, "b.txt").await;
    harness.press("Shift+F8").await;
    let dialog = harness.app.file_ops.dialog.clone().expect("the question");
    assert_eq!(dialog.kind, FileDialog::DeletePermanent);
    assert!(
        Path::new(&root.path().join("b.txt")).exists(),
        "nothing before OK"
    );
    harness.press("Enter").await;
    let job = last_job(&harness);
    finish(&mut harness, job).await;
    assert!(!root.path().join("b.txt").exists());
    assert_eq!(names(&harness, PanelId::Left), ["out", "a.txt"]);
}

#[tokio::test]
async fn the_function_key_bar_runs_the_key_s_command() {
    let (_root, mut harness) = setup().await;
    cursor_to(&mut harness, "a.txt").await;
    let copy = Command::FileDialog {
        panel: None,
        dialog: FileDialog::Copy,
    };
    harness.send(send(FileOpsMsg::Key(Box::new(copy)))).await;
    assert!(harness.app.file_ops.dialog.is_some());
}

/// Tab and Shift+Tab walk the copy dialog's fields - target, filter - and reach no panel
/// (RC-080-05, Windows test 2026-10-18).
#[tokio::test]
async fn tab_walks_the_fields_of_the_copy_dialog() {
    let (_root, mut harness) = setup().await;
    cursor_to(&mut harness, "a.txt").await;
    harness.press("F5").await;
    harness.press("Tab").await;
    harness.press("Shift+Tab").await;
    assert_eq!(harness.fields_walked, [false, true]);
    assert_eq!(
        harness.app.active,
        PanelId::Left,
        "Tab did not reach the panels"
    );
    assert!(
        harness.app.file_ops.dialog.is_some(),
        "the dialog stays open"
    );
}

/// RC-010-26: F2 in a job's progress window sends the job to the background, as Esc does - the
/// same key that queues it in the copy dialog.
#[tokio::test]
async fn f2_sends_a_job_in_its_progress_window_to_the_background() {
    let (_root, mut harness) = setup().await;
    let snapshot = rc_engine::JobSnapshot {
        id: 999,
        kind: rc_engine::JobKind::Copy {
            sources: vec![rc_engine::VfsPath::local("/src/a.txt")],
            target: rc_engine::VfsPath::local("/dst"),
        },
        options: rc_engine::JobOptions::default(),
        state: rc_engine::JobState::Running,
        progress: rc_engine::Progress::default(),
        question: None,
        skipped: 0,
        failures: Vec::new(),
        report: None,
        resumable: false,
        batch: None,
    };
    let follow = super::dialog::FollowUp::Refresh;
    let _ = harness
        .app
        .file_ops
        .jobs
        .adopt(snapshot, PanelId::Left, follow, true);
    assert!(harness.app.file_ops.jobs.foreground().is_some());
    harness.press("F2").await;
    assert!(harness.app.file_ops.jobs.foreground().is_none());
    assert!(!harness.app.file_ops.modal());
}
