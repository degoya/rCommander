//! Drag and drop through `apply`, without a window (RC-080-01): the gesture from a press on a
//! row to the release over the other panel, files dropped onto a panel, the fetch question and
//! the temporary folder's end.

use std::{fs, path::Path, time::Duration};

use iced::{Point, keyboard::Modifiers};
use rc_engine::{Command, FileDialog, JobId, PanelId};
use tempfile::TempDir;

use super::*;
use crate::app::{
    file_list::ColumnWidths, file_ops::FileOpsMsg, harness::Harness, resize::ResizeMsg,
};

/// `root/` with `a.txt`, `b.txt` and `out/`; the left panel on `root`, the right one on `out`.
async fn setup() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("out")).expect("mkdir");
    fs::write(root.path().join("a.txt"), b"a").expect("write");
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

async fn run(harness: &mut Harness, command: Command) {
    harness
        .app
        .engine()
        .dispatch(command)
        .await
        .expect("dispatched");
    harness.settle().await;
}

/// The row of `name` in `panel`, the `..` row counted.
fn row_of(harness: &Harness, panel: PanelId, name: &str) -> usize {
    let pane = harness.app.pane(panel);
    let index = pane
        .entries()
        .iter()
        .position(|entry| entry.name == name)
        .expect("listed");
    index + usize::from(pane.has_parent_row())
}

async fn drag(harness: &mut Harness, message: DragMsg) {
    harness.send(Message::Drag(message)).await;
}

/// A press on `row` of the left panel and a move far enough to start a drag.
async fn drag_from(harness: &mut Harness, row: usize) {
    harness.send(Message::Click(PanelId::Left, row)).await;
    drag(harness, DragMsg::Moved(Point::new(100.0, 100.0))).await;
    drag(harness, DragMsg::Moved(Point::new(140.0, 100.0))).await;
}

/// Waits until the newest job is over and lets the app see its end.
async fn finish_last(harness: &mut Harness) -> JobId {
    let id = harness
        .app
        .engine()
        .ops()
        .jobs()
        .last()
        .map(|job| job.id)
        .expect("a job");
    tokio::time::timeout(Duration::from_secs(10), harness.app.engine().ops().wait(id))
        .await
        .expect("in time")
        .expect("known");
    harness.settle().await;
    id
}

#[tokio::test]
async fn a_drag_onto_the_other_panel_opens_the_copy_dialog_for_the_entry() {
    let (root, mut harness) = setup().await;
    let row = row_of(&harness, PanelId::Left, "b.txt");
    drag_from(&mut harness, row).await;
    harness.send(Message::Hover(PanelId::Right)).await;
    drag(&mut harness, DragMsg::Released).await;
    let dialog = harness
        .app
        .file_ops
        .dialog
        .clone()
        .expect("the copy dialog");
    assert_eq!(dialog.kind, FileDialog::Copy);
    assert_eq!(dialog.count, 1);
    assert_eq!(dialog.single.as_deref(), Some("b.txt"));
    assert!(
        dialog
            .text
            .starts_with(&root.path().join("out").display().to_string())
    );
    assert!(!harness.app.drag.tracking(), "the gesture is over");
}

#[tokio::test]
async fn shift_at_the_release_opens_the_move_dialog() {
    let (_root, mut harness) = setup().await;
    let row = row_of(&harness, PanelId::Left, "a.txt");
    drag_from(&mut harness, row).await;
    harness.send(Message::Hover(PanelId::Right)).await;
    harness
        .send(Message::FileOps(FileOpsMsg::Modifiers(Modifiers::SHIFT)))
        .await;
    drag(&mut harness, DragMsg::Released).await;
    let dialog = harness
        .app
        .file_ops
        .dialog
        .clone()
        .expect("the move dialog");
    assert_eq!(dialog.kind, FileDialog::Move);
}

#[tokio::test]
async fn a_selected_entry_drags_the_selection() {
    let (_root, mut harness) = setup().await;
    run(
        &mut harness,
        Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["a.txt".into(), "b.txt".into()],
        },
    )
    .await;
    let row = row_of(&harness, PanelId::Left, "a.txt");
    drag_from(&mut harness, row).await;
    harness.send(Message::Hover(PanelId::Right)).await;
    drag(&mut harness, DragMsg::Released).await;
    let dialog = harness
        .app
        .file_ops
        .dialog
        .clone()
        .expect("the copy dialog");
    assert_eq!(dialog.count, 2);
    assert_eq!(dialog.single, None);
}

#[tokio::test]
async fn a_short_move_is_a_click_and_drags_nothing() {
    let (_root, mut harness) = setup().await;
    let row = row_of(&harness, PanelId::Left, "a.txt");
    harness.send(Message::Click(PanelId::Left, row)).await;
    drag(&mut harness, DragMsg::Moved(Point::new(100.0, 100.0))).await;
    drag(&mut harness, DragMsg::Moved(Point::new(103.0, 102.0))).await;
    harness.send(Message::Hover(PanelId::Right)).await;
    drag(&mut harness, DragMsg::Released).await;
    assert!(harness.app.file_ops.dialog.is_none());
    assert!(!harness.app.drag.tracking());
}

#[tokio::test]
async fn a_release_over_the_same_panel_or_the_parent_row_does_nothing() {
    let (_root, mut harness) = setup().await;
    let row = row_of(&harness, PanelId::Left, "a.txt");
    drag_from(&mut harness, row).await;
    drag(&mut harness, DragMsg::Released).await;
    assert!(harness.app.file_ops.dialog.is_none(), "over its own panel");
    assert!(harness.app.pane(PanelId::Left).has_parent_row());
    drag_from(&mut harness, 0).await;
    assert!(!harness.app.drag.tracking(), "`..` is no entry to drag");
    harness.send(Message::Hover(PanelId::Right)).await;
    drag(&mut harness, DragMsg::Released).await;
    assert!(harness.app.file_ops.dialog.is_none());
}

#[tokio::test]
async fn a_new_press_ends_a_drag_that_missed_its_release() {
    let (_root, mut harness) = setup().await;
    let row = row_of(&harness, PanelId::Left, "a.txt");
    drag_from(&mut harness, row).await;
    harness.send(Message::Press).await;
    assert!(!harness.app.drag.tracking());
    harness.send(Message::Hover(PanelId::Right)).await;
    drag(&mut harness, DragMsg::Released).await;
    assert!(harness.app.file_ops.dialog.is_none());
}

#[tokio::test]
async fn files_dropped_onto_a_panel_are_copied_into_its_folder() {
    let (root, mut harness) = setup().await;
    let elsewhere = tempfile::tempdir().expect("source");
    let file = elsewhere.path().join("dropped.txt");
    fs::write(&file, b"d").expect("write");
    drag(&mut harness, DragMsg::FileDropped(file.clone())).await;
    drag(&mut harness, DragMsg::FileDropped(file.clone())).await;
    let pointer = Pointer {
        across: 0.8,
        shift: false,
    };
    drag(&mut harness, DragMsg::Settled(Some(pointer))).await;
    finish_last(&mut harness).await;
    assert_eq!(
        fs::read(root.path().join("out/dropped.txt")).expect("copied"),
        b"d"
    );
    assert!(file.exists(), "a copy, not a move");
}

#[tokio::test]
async fn shift_at_a_drop_moves_into_the_hovered_panel_without_a_pointer() {
    let (root, mut harness) = setup().await;
    let elsewhere = tempfile::tempdir().expect("source");
    let file = elsewhere.path().join("moved.txt");
    fs::write(&file, b"m").expect("write");
    harness.send(Message::Hover(PanelId::Right)).await;
    harness
        .send(Message::FileOps(FileOpsMsg::Modifiers(Modifiers::SHIFT)))
        .await;
    drag(&mut harness, DragMsg::FileDropped(file.clone())).await;
    drag(&mut harness, DragMsg::Settled(None)).await;
    finish_last(&mut harness).await;
    assert!(root.path().join("out/moved.txt").exists());
    assert!(!file.exists(), "moved");
}

#[tokio::test]
async fn hostile_or_pointless_drops_start_no_job() {
    let (root, mut harness) = setup().await;
    drag(&mut harness, DragMsg::FileDropped("relative/x".into())).await;
    drag(
        &mut harness,
        DragMsg::FileDropped(root.path().join("../escape")),
    )
    .await;
    let left = Pointer {
        across: 0.2,
        shift: false,
    };
    drag(&mut harness, DragMsg::Settled(Some(left))).await;
    assert!(harness.app.info.is_some(), "the refused paths are counted");
    // A file dropped into the folder it lies in.
    drag(
        &mut harness,
        DragMsg::FileDropped(root.path().join("a.txt")),
    )
    .await;
    drag(&mut harness, DragMsg::Settled(Some(left))).await;
    assert!(harness.app.engine().ops().jobs().is_empty());
}

#[tokio::test]
async fn the_fetch_question_has_the_keys() {
    let (_root, mut harness) = setup().await;
    let state = harness.app.pane(PanelId::Left).state.clone();
    let question = Fetch::new(PanelId::Left, &state, vec!["out".into()], Purpose::Drag);
    assert!(question.asks(), "an uncounted folder");
    harness.app.drag.question = Some(question.clone());
    harness.press("ArrowDown").await;
    assert!(harness.app.drag.question.is_some(), "the panel has no keys");
    harness.press("Escape").await;
    assert!(harness.app.drag.question.is_none());
    assert!(harness.app.drag.running.is_none(), "nothing fetched");
    harness.app.drag.question = Some(question);
    harness.press("Enter").await;
    assert!(harness.app.drag.question.is_none());
    assert!(
        harness.app.drag.running.is_some(),
        "the fetch waits for its folder"
    );
}

/// With the dialogs in windows the fetch question is a modal window of its own (RC-080-23): no
/// card in the main window, its keys reach it while the main window has the keyboard, Escape
/// cancels it and its window closes next.
#[tokio::test]
async fn the_fetch_question_has_a_window_of_its_own() {
    use crate::app::dialog_windows::{DialogWindow, DialogWindows};
    let (_root, mut harness) = setup().await;
    let main = iced::window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    harness.send(Message::DialogWindows(true)).await;
    let state = harness.app.pane(PanelId::Left).state.clone();
    let question = Fetch::new(PanelId::Left, &state, vec!["out".into()], Purpose::Drag);
    harness.app.drag.question = Some(question);
    let wanted = harness.app.wanted_windows();
    let window = *wanted.last().expect("a window");
    assert!(window.is_question() && window.modal(), "{window:?}");
    assert!(harness.app.main_locked());
    assert!(
        harness
            .app
            .drag_question_body(crate::theme::tokens(false))
            .is_some()
    );

    harness.press("ArrowDown").await;
    assert!(harness.app.drag.question.is_some(), "the panel has no keys");
    harness.press("Escape").await;
    assert!(harness.app.drag.question.is_none(), "cancelled");
    assert!(
        !harness
            .app
            .wanted_windows()
            .iter()
            .any(|kind| matches!(kind, DialogWindow::Question(_))),
        "its window closes next"
    );
}

#[tokio::test]
async fn a_finished_fetch_is_kept_for_the_next_drag() {
    let (root, mut harness) = setup().await;
    let temp = tempfile::tempdir().expect("temp root");
    let session = temp::make_session(temp.path()).expect("session");
    let state = harness.app.pane(PanelId::Left).state.clone();
    let fetch = Fetch::new(PanelId::Left, &state, vec!["a.txt".into()], Purpose::Drag);
    let key = fetch.key.clone();
    harness.app.drag.temp.set_session(session.clone());
    let mut effects = harness.app.submit_fetch(fetch);
    let Some(Effect::Ops(submit)) = effects.pop() else {
        panic!("the fetch is a job");
    };
    let done = submit.run(harness.app.engine().clone()).await;
    harness
        .send(Message::FileOps(FileOpsMsg::Submitted(Box::new(done))))
        .await;
    finish_last(&mut harness).await;
    let folder = session.join("1");
    assert_eq!(fs::read(folder.join("a.txt")).expect("fetched"), b"a");
    assert_eq!(harness.app.drag.fetched.get(&key), Some(&folder));
    assert!(harness.app.drag.running.is_none());
    assert!(root.path().join("a.txt").exists(), "the source stays");
}

#[tokio::test]
async fn the_apps_end_removes_the_temporary_folder() {
    let (_root, mut harness) = setup().await;
    let temp = tempfile::tempdir().expect("temp root");
    let session = temp::make_session(temp.path()).expect("session");
    fs::write(session.join("f"), b"f").expect("write");
    harness.app.drag.temp.set_session(session.clone());
    harness.send(Message::Saved).await;
    assert!(
        harness.app.drag.temp.session().is_none(),
        "the folder is handed to the removal"
    );
    temp::remove_off_thread(session.clone()).await;
    assert!(!Path::new(&session).exists());
}

/// `file`, from another folder, dropped at `across` of the window's width.
async fn drop_at(harness: &mut Harness, file: &Path, across: f32) {
    drag(harness, DragMsg::FileDropped(file.to_owned())).await;
    let pointer = Pointer {
        across,
        shift: false,
    };
    drag(harness, DragMsg::Settled(Some(pointer))).await;
}

/// RC-080-38: a drop lands on its side of the divider, not of the window's middle; while a
/// panel is maximized, in that one wherever it lands.
#[tokio::test]
async fn a_drop_lands_on_the_side_of_the_divider_or_in_the_maximized_panel() {
    let (root, mut harness) = setup().await;
    let divider = ResizeMsg::Loaded(ColumnWidths::default(), 300.0);
    harness.send(Message::Resize(divider)).await;
    assert_eq!(harness.app.resize.left_share(), 300);
    let elsewhere = tempfile::tempdir().expect("source");
    let file = elsewhere.path().join("dropped.txt");
    fs::write(&file, b"d").expect("write");

    harness.app.pane_mut(PanelId::Left).state.view = PanelView::Terminal;
    harness.app.terms.maximized = Some(PanelId::Left);
    assert_eq!(harness.app.maximized(), Some(PanelId::Left));
    drop_at(&mut harness, &file, 0.9).await;
    assert_eq!(
        harness.app.pane(PanelId::Left).error.as_deref(),
        Some(t!("drop.files_only")),
        "the maximized terminal, not the right panel under it"
    );
    assert!(harness.app.engine().ops().jobs().is_empty());

    harness.app.pane_mut(PanelId::Left).state.view = PanelView::Files;
    harness.app.terms.maximized = None;
    drop_at(&mut harness, &file, 0.4).await;
    finish_last(&mut harness).await;
    assert!(
        root.path().join("out/dropped.txt").exists(),
        "the right panel"
    );
    assert!(!root.path().join("dropped.txt").exists());
}

/// RC-080-38: a second fetch while the session folder is being made asks for no second folder,
/// and a folder made besides is not taken - one job, the session keeps its first folder.
#[tokio::test]
async fn two_fetches_before_the_session_folder_start_one_job() {
    let (_root, mut harness) = setup().await;
    let temp = tempfile::tempdir().expect("temp root");
    let first = temp::make_session(temp.path()).expect("session");
    let besides = temp::make_session(temp.path()).expect("another");
    let state = harness.app.pane(PanelId::Left).state.clone();
    let new_fetch = || Fetch::new(PanelId::Left, &state, vec!["a.txt".into()], Purpose::Drag);
    let asks = |effects: Vec<Effect>| {
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Task(_)))
            .count()
    };
    assert_eq!(asks(harness.app.begin_fetch(new_fetch())), 1, "a folder");
    assert_eq!(asks(harness.app.begin_fetch(new_fetch())), 0, "not two");
    drag(&mut harness, DragMsg::Session(Ok(first.clone()))).await;
    drag(&mut harness, DragMsg::Session(Ok(besides))).await;
    finish_last(&mut harness).await;
    assert_eq!(harness.app.engine().ops().jobs().len(), 1, "one job");
    assert_eq!(harness.app.drag.temp.session(), Some(first.as_path()));
    assert_eq!(fs::read(first.join("1/a.txt")).expect("fetched"), b"a");
    assert!(harness.app.drag.running.is_none());
}
