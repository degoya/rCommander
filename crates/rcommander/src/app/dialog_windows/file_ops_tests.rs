//! The file operations in windows of their own through `apply` (RC-080-23, stage 2): each dialog
//! of F5, F6, Shift+F5, F7, Shift+F4, F8 and Shift+F8 opens a modal window its keys reach, a
//! job's conflict and its progress have windows of their own - the question on top, also over
//! another modal dialog's window -, Escape and the title bar's X act in the window they are in,
//! Background returns the job to the strip, a question's window comes to the front for a new
//! question, and a small dialog's window is hidden and taken back. The setting off keeps the
//! cards. The windows themselves need a display; their books are `book_tests.rs`, the rings
//! `field_walk/focus_ring_windows_tests.rs`.

use std::time::{Duration, Instant};

use iced::window;
use rc_engine::{Chord, EngineEvent, FileDialog, JobId, PanelId};

use crate::{
    app::{
        Effect, Message,
        dialog_windows::{DialogWindow, DialogWindowMsg, DialogWindows},
        file_ops::FileOpsMsg,
        harness::Harness,
        queue_window::QueueMsg,
    },
    theme,
};

/// The app with `diff.txt` on both sides under the cursor - a copy asks - with
/// `appearance.dialog_windows` as `own` and its main window `main`.
async fn app(own: bool) -> (tempfile::TempDir, Harness, window::Id) {
    let root = tempfile::tempdir().expect("temp dir");
    for side in ["left", "right"] {
        let dir = root.path().join(side);
        std::fs::create_dir(&dir).expect("mkdir");
        std::fs::write(dir.join("diff.txt"), side).expect("write");
    }
    let mut harness = Harness::open(&root.path().join("left"), &root.path().join("right")).await;
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    harness.send(Message::DialogWindows(own)).await;
    harness.press("ArrowDown").await;
    (root, harness, main)
}

/// The window `sync_dialog_windows` would open for `kind`, booked as shown and with the
/// keyboard: its id.
async fn window_of(harness: &mut Harness, main: window::Id, kind: DialogWindow) -> window::Id {
    let wanted = harness.app.wanted_windows();
    let books = &mut harness.app.dialog_windows;
    let (opening, _) = books.difference(&wanted);
    assert!(
        opening.contains(&kind),
        "a window for {kind:?}: {opening:?}"
    );
    let id = window::Id::unique();
    let geometry = books.geometry(kind, harness.app.window.geometry());
    books.opened(kind, id, geometry);
    focus(harness, main, id).await;
    id
}

async fn focus(harness: &mut Harness, from: window::Id, to: window::Id) {
    for (window, focused) in [(from, false), (to, true)] {
        harness
            .send(Message::Queue(QueueMsg::Focused(window, focused)))
            .await;
    }
}

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

/// The windows closing next.
fn closing(harness: &Harness) -> Vec<window::Id> {
    let wanted = harness.app.wanted_windows();
    harness.app.dialog_windows.difference(&wanted).1
}

/// F5 onto the other side's file, OK or Queue (`background`): the job, once it asks.
async fn asking_copy(harness: &mut Harness, background: bool) -> JobId {
    let copy = EngineEvent::FileDialog {
        panel: PanelId::Left,
        dialog: FileDialog::Copy,
    };
    harness.send(Message::Engine(copy)).await;
    harness
        .send(Message::FileOps(FileOpsMsg::Start { background }))
        .await;
    let engine = harness.app.engine().clone();
    let job = engine.ops().jobs().last().map(|job| job.id).expect("a job");
    for _ in 0..500 {
        if engine.ops().job(job).expect("known").question.is_some() {
            harness.settle().await;
            assert!(harness.app.file_ops.jobs.asking().is_some(), "asked");
            return job;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {job} asked nothing");
}

/// Waits until job `job` is over and lets the app see it.
async fn ended(harness: &mut Harness, job: JobId) {
    let wait = harness.app.engine().ops().wait(job);
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .expect("in time")
        .expect("known");
    harness.settle().await;
}

const DIALOGS: [(&str, DialogWindow); 7] = [
    ("F5", DialogWindow::Transfer),
    ("F6", DialogWindow::Transfer),
    ("Shift+F5", DialogWindow::Transfer),
    ("F7", DialogWindow::NewItem),
    ("Shift+F4", DialogWindow::NewItem),
    ("F8", DialogWindow::Delete),
    ("Shift+F8", DialogWindow::Delete),
];

/// Each file operation's dialog opens a modal window of its own with its title and content,
/// locking the main window; Tab walks its fields there, Escape closes it and its window follows.
#[tokio::test]
async fn each_file_dialog_has_a_modal_window_its_keys_reach() {
    let tokens = theme::tokens(false);
    for (key, kind) in DIALOGS {
        let (_root, mut harness, main) = app(true).await;
        harness.press(key).await;
        assert!(harness.app.file_ops.dialog.is_some(), "{key} opens");
        assert_eq!(harness.app.wanted_windows(), [kind], "{key}");
        let id = window_of(&mut harness, main, kind).await;
        assert!(kind.modal() && harness.app.main_locked(), "{key} is modal");
        assert!(
            !harness.app.file_ops_title(kind).is_empty(),
            "{key}'s title"
        );
        assert!(harness.app.file_ops_body(kind, tokens).is_some());

        let effects = harness.app.apply(Message::Key(chord("Tab")));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::WalkFields { back: false, .. })),
            "Tab walks {key}'s window"
        );
        harness.press("Escape").await;
        assert!(harness.app.file_ops.dialog.is_none(), "Escape closes {key}");
        assert_eq!(closing(&harness), [id], "{key}'s window closes next");
    }
}

/// The title bar's X and F10 close each dialog as Escape does; none of them quits.
#[tokio::test]
async fn the_x_and_f10_close_each_file_dialog() {
    for (key, kind) in DIALOGS {
        let (_root, mut harness, main) = app(true).await;
        harness.press(key).await;
        let id = window_of(&mut harness, main, kind).await;
        harness.send(Message::CloseRequested(id)).await;
        assert!(harness.app.file_ops.dialog.is_none(), "the X closes {key}");
        assert!(harness.app.dialog_windows.closed(id).is_some());

        focus(&mut harness, id, main).await;
        harness.press(key).await;
        let _ = window_of(&mut harness, main, kind).await;
        harness.press("F10").await;
        assert!(harness.app.file_ops.dialog.is_none(), "F10 closes {key}");
        assert!(!harness.app.quit.leaving, "and quits nothing");
    }
}

/// With the setting off every file operation's dialog is a card over the main window, as
/// before, and its question too.
#[tokio::test]
async fn off_the_file_operations_stay_cards() {
    for (key, _) in DIALOGS {
        let (_root, mut harness, _main) = app(false).await;
        harness.press(key).await;
        assert!(harness.app.wanted_windows().is_empty(), "{key} a card");
        assert!(!harness.app.main_locked());
        harness.press("Escape").await;
        assert!(harness.app.file_ops.dialog.is_none(), "Escape closes {key}");
    }
    let (_root, mut harness, _main) = app(false).await;
    let job = asking_copy(&mut harness, false).await;
    assert!(
        harness.app.wanted_windows().is_empty(),
        "the question a card"
    );
    assert_eq!(harness.app.question_window(), None);
    harness.press("Escape").await;
    ended(&mut harness, job).await;
}

/// A foreground copy that asks: its progress window and its question, the question first. The
/// title bar's X of the progress window sends the job to the background - the question stays,
/// now a background job's -, the question's X cancels the question.
#[tokio::test]
async fn the_x_of_each_window_acts_in_that_window() {
    let (_root, mut harness, main) = app(true).await;
    let job = asking_copy(&mut harness, false).await;
    assert_eq!(
        harness.app.wanted_windows(),
        [DialogWindow::Conflict, DialogWindow::Progress],
        "the question on top"
    );
    let progress = window_of(&mut harness, main, DialogWindow::Progress).await;
    let question = window_of(&mut harness, main, DialogWindow::Conflict).await;

    harness.send(Message::CloseRequested(progress)).await;
    assert!(
        harness.app.file_ops.jobs.foreground().is_none(),
        "to the strip"
    );
    assert!(
        harness.app.file_ops.jobs.asking().is_some(),
        "the question stays"
    );
    assert_eq!(closing(&harness), [progress]);
    assert!(harness.app.dialog_windows.closed(progress).is_some());

    harness.send(Message::CloseRequested(question)).await;
    ended(&mut harness, job).await;
    assert!(harness.app.file_ops.jobs.asking().is_none(), "cancelled");
    assert_eq!(closing(&harness), [question]);
}

/// The progress window's Background puts the job into the strip and closes the window; Escape
/// in the question's window cancels the question.
#[tokio::test]
async fn background_returns_the_job_to_the_strip() {
    let (_root, mut harness, main) = app(true).await;
    let job = asking_copy(&mut harness, false).await;
    let progress = window_of(&mut harness, main, DialogWindow::Progress).await;
    let question = window_of(&mut harness, main, DialogWindow::Conflict).await;
    harness
        .send(Message::FileOps(FileOpsMsg::Background(job)))
        .await;
    let jobs = &harness.app.file_ops.jobs;
    assert!(jobs.foreground().is_none());
    assert_eq!(
        jobs.background().map(|job| job.id()).collect::<Vec<_>>(),
        [job]
    );
    assert_eq!(closing(&harness), [progress], "its window closes");

    focus(&mut harness, progress, question).await;
    harness.press("Escape").await;
    ended(&mut harness, job).await;
    assert!(
        harness.app.file_ops.jobs.asking().is_none(),
        "Escape cancels"
    );
}

/// A background job's conflict opens its window while another modal dialog's window - quick
/// connect - is open, on top of it: its keys are the question's while its window has the
/// keyboard, the other dialog's while that one has; the X of the other closes only that.
#[tokio::test]
async fn a_background_question_opens_its_window_over_a_modal_dialog() {
    let (_root, mut harness, main) = app(true).await;
    harness.press("Ctrl+N").await;
    let quick = window_of(&mut harness, main, DialogWindow::QuickConnect).await;
    let job = asking_copy(&mut harness, true).await;
    assert!(
        harness.app.file_ops.jobs.foreground().is_none(),
        "in the strip"
    );
    assert_eq!(
        harness.app.wanted_windows(),
        [DialogWindow::Conflict, DialogWindow::QuickConnect],
        "the question's window opens over the dialog's"
    );
    let _ = harness.app.sync_dialog_windows();
    let question = harness
        .app
        .dialog_windows
        .id_of(DialogWindow::Conflict)
        .expect("its window is made");
    let since = Instant::now();
    let ready = DialogWindowMsg::Ready {
        id: question,
        since,
        reused: false,
    };
    assert!(
        !harness.app.on_dialog_window(ready).is_empty(),
        "placed and shown"
    );
    let shown = DialogWindowMsg::Shown {
        id: question,
        since,
        reused: false,
    };
    harness.send(Message::DialogWindow(shown)).await;

    // The question's window not yet focused: the quick connection keeps its keys.
    assert!(!harness.app.job_question_has_keys());
    assert!(!harness.app.file_ops_first(&Message::Key(chord("Escape"))));
    harness.send(Message::CloseRequested(quick)).await;
    assert!(harness.app.quick.is_none(), "the X closes quick connect");
    assert!(
        harness.app.file_ops.jobs.asking().is_some(),
        "not the question"
    );

    focus(&mut harness, quick, question).await;
    assert!(harness.app.job_question_has_keys());
    harness.press("Escape").await;
    ended(&mut harness, job).await;
    assert!(
        harness.app.file_ops.jobs.asking().is_none(),
        "Escape cancels it"
    );
}

/// In its window a question comes to the front for each new question while its window is
/// shown, never the main window; a window just opening shows in front by itself.
#[tokio::test]
async fn a_new_question_brings_its_window_to_the_front() {
    let (_root, mut harness, main) = app(true).await;
    let job = asking_copy(&mut harness, true).await;
    let question = window_of(&mut harness, main, DialogWindow::Conflict).await;
    assert_eq!(harness.app.front_for_question(), Some(question), "new");
    assert_eq!(harness.app.front_for_question(), None, "once");
    harness.press("Escape").await;
    ended(&mut harness, job).await;
    assert_eq!(harness.app.front_for_question(), None);
}

/// A small dialog's window is hidden when the dialog closes and taken back at its next opening;
/// a window is booked as shown only once it is, and closes only then. The progress window is
/// made for each job.
#[tokio::test]
async fn a_small_dialog_window_is_hidden_and_taken_back() {
    let (_root, mut harness, _main) = app(true).await;
    assert!(DialogWindow::NewItem.reused() && !DialogWindow::Progress.reused());
    harness.press("F7").await;
    let _ = harness.app.sync_dialog_windows();
    let books = &harness.app.dialog_windows;
    let id = books.id_of(DialogWindow::NewItem).expect("being made");
    assert_eq!(books.visible(DialogWindow::NewItem), None, "not shown yet");
    let since = Instant::now();
    for message in [
        DialogWindowMsg::Ready {
            id,
            since,
            reused: false,
        },
        DialogWindowMsg::Shown {
            id,
            since,
            reused: false,
        },
    ] {
        harness.send(Message::DialogWindow(message)).await;
    }
    assert_eq!(
        harness.app.dialog_windows.visible(DialogWindow::NewItem),
        Some(id)
    );

    harness.press("Escape").await;
    assert_eq!(closing(&harness), [id]);
    let _ = harness.app.sync_dialog_windows();
    assert_eq!(harness.app.dialog_windows.kind_of(id), None, "hidden");

    harness.press("F7").await;
    let _ = harness.app.sync_dialog_windows();
    assert_eq!(
        harness.app.dialog_windows.id_of(DialogWindow::NewItem),
        Some(id),
        "the same window again"
    );
}

/// A dialog that closes before its window is ready lets the window go unshown: it never locked
/// the main window.
#[tokio::test]
async fn a_window_whose_dialog_closed_goes_unshown() {
    let (_root, mut harness, _main) = app(true).await;
    harness.press("F8").await;
    let _ = harness.app.sync_dialog_windows();
    let id = harness
        .app
        .dialog_windows
        .id_of(DialogWindow::Delete)
        .expect("being made");
    harness.press("Escape").await;
    assert!(closing(&harness).is_empty(), "not before it is ready");
    assert!(
        !harness.app.dialog_windows.modal_open(),
        "it locked nothing"
    );
    let ready = DialogWindowMsg::Ready {
        id,
        since: Instant::now(),
        reused: false,
    };
    harness.send(Message::DialogWindow(ready)).await;
    assert_eq!(harness.app.dialog_windows.kind_of(id), None, "gone unshown");
    harness.press("F8").await;
    let _ = harness.app.sync_dialog_windows();
    assert_eq!(
        harness.app.dialog_windows.id_of(DialogWindow::Delete),
        Some(id),
        "and kept hidden for the next one"
    );
}
