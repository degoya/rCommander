//! The queue in a window of its own through `apply` (RC-010-14): modeless - the main window keeps
//! its keys while the queue's window does not have the keyboard -, brought to the front by
//! Ctrl+Shift+Q, closed by Escape, F10 and its X with the queue running on, its place kept. The
//! windows themselves need a display; their books are `dialog_windows/book_tests.rs`.

use iced::{Point, window};
use rc_engine::{Command, EngineEvent};

use crate::app::{
    Effect, Message,
    dialog_windows::{DialogWindow, DialogWindows},
    harness::Harness,
    queue_window::QueueMsg,
    window_state::WindowChange,
};

/// The app with `queue.window_own` on, its main window `main`, and the queue open in the window
/// `sync_dialog_windows` would open for it: its id.
async fn queue_in_its_window() -> (tempfile::TempDir, Harness, window::Id, window::Id) {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::write(root.path().join("a.txt"), b"a").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    harness.send(Message::Queue(QueueMsg::Own(true))).await;
    harness
        .app
        .engine()
        .dispatch(Command::QueueWindow {})
        .await
        .expect("opens");
    harness.settle().await;
    assert!(harness.app.queue_window.is_some());
    assert_eq!(harness.app.wanted_windows(), [DialogWindow::Queue]);
    let books = &mut harness.app.dialog_windows;
    let (opening, _) = books.difference(&[DialogWindow::Queue]);
    assert_eq!(opening, [DialogWindow::Queue]);
    let id = window::Id::unique();
    let geometry = books.geometry(DialogWindow::Queue, harness.app.window.geometry());
    books.opened(DialogWindow::Queue, id, geometry);
    (root, harness, main, id)
}

async fn focus(harness: &mut Harness, from: window::Id, to: window::Id) {
    harness
        .send(Message::Queue(QueueMsg::Focused(from, false)))
        .await;
    harness
        .send(Message::Queue(QueueMsg::Focused(to, true)))
        .await;
}

/// Modeless: with the main window focused its keys work - F7 opens the new-folder dialog,
/// Escape closes that, not the queue - and the queue counts as no open dialog there.
#[tokio::test]
async fn the_main_window_keeps_its_keys_beside_the_queue_window() {
    let (_root, mut harness, main, id) = queue_in_its_window().await;
    focus(&mut harness, id, main).await;
    assert!(
        !harness.app.dialog_open(),
        "the queue does not hold the main window"
    );

    harness.press("F7").await;
    assert!(harness.app.file_ops.dialog.is_some(), "a panel key");
    harness.press("Escape").await;
    assert!(harness.app.file_ops.dialog.is_none());
    assert!(harness.app.queue_window.is_some(), "the queue stays open");

    // Its own window has the keyboard: its keys, and Escape closes it; the queue runs on.
    focus(&mut harness, main, id).await;
    assert!(harness.app.dialog_open());
    harness.press("Escape").await;
    assert!(harness.app.queue_window.is_none());
    assert!(harness.app.wanted_windows().is_empty());
    let (_, closing) = harness.app.dialog_windows.difference(&[]);
    assert_eq!(closing, [id], "its window closes next");
}

/// F10 in the queue's window closes it as Escape does (`keys.f10` default); the X does too, also
/// while the main window has the keyboard.
#[tokio::test]
async fn f10_and_the_x_close_the_queue_window() {
    let (_root, mut harness, main, id) = queue_in_its_window().await;
    focus(&mut harness, main, id).await;
    harness.press("F10").await;
    assert!(harness.app.queue_window.is_none());
    assert!(!harness.app.quit.leaving, "F10 there quits nothing");

    let (_root, mut harness, main, id) = queue_in_its_window().await;
    focus(&mut harness, id, main).await;
    harness.send(Message::CloseRequested(id)).await;
    assert!(harness.app.queue_window.is_none());
    assert!(!harness.app.quit.leaving);
}

/// Ctrl+Shift+Q with the queue open in its window brings that window to the front; lying over
/// the main window it asks nothing of the system.
#[tokio::test]
async fn ctrl_shift_q_brings_the_open_queue_window_to_the_front() {
    let (_root, mut harness, _main, id) = queue_in_its_window().await;
    let effects = harness.app.apply(Message::Engine(EngineEvent::QueueWindow));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Task(_))),
        "to the front"
    );
    assert!(harness.app.queue_window.is_some(), "still open");

    harness.send(Message::Queue(QueueMsg::Own(false))).await;
    assert!(
        harness.app.wanted_windows().is_empty(),
        "over the main window"
    );
    // As `sync_dialog_windows` closes it.
    assert!(harness.app.dialog_windows.closed(id).is_some());
    let effects = harness.app.apply(Message::Engine(EngineEvent::QueueWindow));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Task(_)))
    );
}

/// Where the queue window was left is where it opens next.
#[tokio::test]
async fn the_queue_window_opens_where_it_was_left() {
    let (_root, mut harness, _main, id) = queue_in_its_window().await;
    let moved = WindowChange::Moved(Point::new(300.0, 200.0));
    harness.send(Message::WindowSeen(id, moved, false)).await;
    let books = &mut harness.app.dialog_windows;
    let (kind, left) = books.closed(id).expect("a dialog's window");
    assert_eq!(kind, DialogWindow::Queue);
    assert_eq!(left.position, Some((300.0, 200.0)));
    let main = harness.app.window.geometry();
    let again = harness
        .app
        .dialog_windows
        .geometry(DialogWindow::Queue, main);
    assert_eq!(again, left);
}
