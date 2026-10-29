//! The large dialogs in windows of their own through `apply` (RC-080-15): modeless ones take the
//! keys only while their window has the keyboard, the panels keep theirs beside them, the X is
//! Escape in that window, the questions draw where the keyboard is, and the setting switches it
//! all back to the cards over the main window; a file operation's question as a card brings the
//! main window to the front. The windows themselves need a display; their books are
//! `book_tests.rs`, Lister's windows `viewer/window_tests.rs`.

use std::time::Duration;

use iced::window;
use rc_engine::{Command, CursorTarget, EngineEvent, FileDialog, JobId, PanelId};

use crate::app::{
    Effect, Message,
    checksum::ChecksumWindow,
    dialog_windows::{DialogWindow, DialogWindows},
    file_ops::FileOpsMsg,
    harness::Harness,
    queue_window::QueueMsg,
};

/// The app with `appearance.dialog_windows` on and its main window `main`.
async fn with_windows() -> (tempfile::TempDir, Harness, window::Id) {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::write(root.path().join("a.txt"), b"a").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    harness.send(Message::DialogWindows(true)).await;
    (root, harness, main)
}

/// The window `sync_dialog_windows` would open for `kind`, booked: its id.
fn open_window(harness: &mut Harness, kind: DialogWindow) -> window::Id {
    let wanted = harness.app.wanted_windows();
    let books = &mut harness.app.dialog_windows;
    let (opening, _) = books.difference(&wanted);
    assert_eq!(opening, [kind], "a window for {kind:?}");
    let id = window::Id::unique();
    let geometry = books.geometry(kind, harness.app.window.geometry());
    books.opened(kind, id, geometry);
    id
}

async fn focus(harness: &mut Harness, from: window::Id, to: window::Id) {
    harness
        .send(Message::Queue(QueueMsg::Focused(from, false)))
        .await;
    harness
        .send(Message::Queue(QueueMsg::Focused(to, true)))
        .await;
}

fn search_open(harness: &Harness) -> bool {
    harness
        .app
        .search
        .as_ref()
        .is_some_and(|dialog| dialog.open)
}

/// Find files in its window is modeless: with the main window focused F7 opens the new-folder
/// dialog and Escape closes that, not the search; with its own window focused Escape closes it.
#[tokio::test]
async fn the_main_window_keeps_its_keys_beside_the_find_files_window() {
    let (_root, mut harness, main) = with_windows().await;
    harness.press("Alt+F7").await;
    assert!(search_open(&harness));
    let id = open_window(&mut harness, DialogWindow::Search);
    focus(&mut harness, id, main).await;
    assert!(
        !harness.app.dialog_open(),
        "the search holds no main window"
    );

    harness.press("F7").await;
    assert!(harness.app.file_ops.dialog.is_some(), "a panel key");
    harness.press("Escape").await;
    assert!(harness.app.file_ops.dialog.is_none());
    assert!(search_open(&harness), "the search stays open");

    focus(&mut harness, main, id).await;
    assert!(harness.app.dialog_open());
    harness.press("F7").await;
    assert!(
        harness.app.file_ops.dialog.is_none(),
        "the search window's keys are no panel's"
    );
    harness.press("Escape").await;
    assert!(!search_open(&harness));
    let (_, closing) = harness
        .app
        .dialog_windows
        .difference(&harness.app.wanted_windows());
    assert_eq!(closing, [id], "its window closes next");
}

/// The title bar's X is Escape in that window, also while the main window has the keyboard; the
/// main window's F10 quits beside it, the dialog window's closes it.
#[tokio::test]
async fn the_x_and_f10_close_the_dialog_of_their_window() {
    let (_root, mut harness, main) = with_windows().await;
    harness.press("Ctrl+F").await;
    assert!(harness.app.connections.is_some());
    let id = open_window(&mut harness, DialogWindow::Connections);
    focus(&mut harness, id, main).await;
    harness.send(Message::CloseRequested(id)).await;
    assert!(
        harness.app.connections.is_none(),
        "the X closes the manager"
    );
    assert!(!harness.app.quit.leaving);

    let (_root, mut harness, main) = with_windows().await;
    harness.press("Alt+F7").await;
    let id = open_window(&mut harness, DialogWindow::Search);
    focus(&mut harness, main, id).await;
    harness.press("F10").await;
    assert!(!search_open(&harness), "F10 in its window closes it");
    assert!(!harness.app.quit.leaving, "and quits nothing");
}

/// Asked for again, a dialog in its window comes to the front; over the main window it asks
/// nothing of the system.
#[tokio::test]
async fn asking_again_brings_the_dialog_window_to_the_front() {
    let (_root, mut harness, main) = with_windows().await;
    harness.press("Ctrl+F").await;
    let id = open_window(&mut harness, DialogWindow::Connections);
    focus(&mut harness, id, main).await;
    let event = Message::Engine(EngineEvent::ConnectionManager);
    let effects = harness.app.apply(event.clone());
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Task(_))),
        "to the front"
    );

    harness.send(Message::DialogWindows(false)).await;
    assert!(
        harness.app.wanted_windows().is_empty(),
        "over the main window"
    );
    assert!(harness.app.dialog_windows.closed(id).is_some());
    let effects = harness.app.apply(event);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Task(_)))
    );
}

/// With the setting off every large dialog lies over the main window and has the keys there,
/// as before (RC-080-15's fallback).
#[tokio::test]
async fn off_the_large_dialogs_lie_over_the_main_window() {
    let (_root, mut harness, _main) = with_windows().await;
    harness.send(Message::DialogWindows(false)).await;
    harness.press("Alt+F7").await;
    assert!(harness.app.wanted_windows().is_empty());
    assert!(harness.app.has_keys(DialogWindow::Search));
    harness.press("F7").await;
    assert!(
        harness.app.file_ops.dialog.is_none(),
        "the search has the keys"
    );
    harness.press("Escape").await;
    assert!(!search_open(&harness));
}

/// The small questions draw over the window that has the keyboard, never behind a dialog's
/// window; a modal dialog's window takes them while it is open. The queue's window takes none.
#[tokio::test]
async fn the_questions_draw_where_the_keyboard_is() {
    let (_root, mut harness, main) = with_windows().await;
    assert_eq!(harness.app.question_home(), None);
    harness
        .send(Message::Engine(EngineEvent::FindFiles {
            panel: PanelId::Left,
        }))
        .await;
    let id = open_window(&mut harness, DialogWindow::Search);
    focus(&mut harness, main, id).await;
    assert_eq!(harness.app.question_home(), Some(DialogWindow::Search));
    focus(&mut harness, id, main).await;
    assert_eq!(harness.app.question_home(), None, "the main window");

    harness.send(Message::Queue(QueueMsg::Own(true))).await;
    harness
        .send(Message::Engine(EngineEvent::QueueWindow))
        .await;
    let queue = open_window(&mut harness, DialogWindow::Queue);
    focus(&mut harness, main, queue).await;
    assert_eq!(harness.app.question_home(), None, "not the queue's window");
}

/// The manager's layers - the login-script editor, "Run command" - draw in its window while it
/// has one; the unlock prompt a connection asked for alone is a small question, not the manager's
/// window (`questions.rs`).
#[tokio::test]
async fn the_manager_takes_its_layers_into_its_window() {
    let (_root, mut harness, _main) = with_windows().await;
    assert_eq!(harness.app.manager_home(), None);
    harness.press("Ctrl+F").await;
    assert_eq!(harness.app.wanted_windows(), [DialogWindow::Connections]);
    assert_eq!(harness.app.manager_home(), Some(DialogWindow::Connections));
    if let Some(manager) = harness.app.connections.as_mut() {
        manager.prompt_only = true;
    }
    assert!(harness.app.wanted_windows().is_empty());
    assert_eq!(harness.app.manager_home(), None);
}

/// The checksum results are a modeless window of their own: the panels keep their keys beside
/// it, Escape in it closes it; a form has a modal window of its own (RC-080-23).
#[tokio::test]
async fn the_checksum_results_have_a_window_beside_the_panels() {
    let (_root, mut harness, main) = with_windows().await;
    harness.app.checksum = Some(ChecksumWindow::Results(Vec::new()));
    let id = open_window(&mut harness, DialogWindow::Checksum);
    focus(&mut harness, id, main).await;
    assert!(!harness.app.dialog_open());
    harness.press("F7").await;
    assert!(harness.app.file_ops.dialog.is_some(), "a panel key");
    harness.press("Escape").await;
    assert!(harness.app.checksum.is_some(), "the results stay");

    focus(&mut harness, main, id).await;
    harness.press("Escape").await;
    assert!(harness.app.checksum.is_none(), "Escape in its window");

    // On `..` there is nothing to checksum (RC-080-38): the cursor on a file.
    let cursor = Command::SetCursor {
        panel: Some(PanelId::Left),
        target: CursorTarget::Name("a.txt".into()),
    };
    run(&mut harness, cursor).await;
    assert!(!harness.app.pane(PanelId::Left).on_parent());
    harness
        .send(Message::Engine(EngineEvent::ChecksumDialog {
            panel: PanelId::Left,
            dialog: rc_engine::ChecksumDialog::Create,
        }))
        .await;
    assert!(harness.app.checksum.is_some(), "the create form");
    assert_eq!(
        harness.app.wanted_windows(),
        [DialogWindow::ChecksumForm],
        "a modal window"
    );
}

/// The login-script editor opens in a modeless window of its own beside the connection
/// manager: each window's keys are its own, Escape closes the dialog of the window that has the
/// keyboard, and the editor no longer draws over the manager.
#[tokio::test]
async fn the_login_script_editor_has_a_window_beside_the_manager() {
    let (_root, mut harness, main) = with_windows().await;
    harness.press("Ctrl+F").await;
    let manager = open_window(&mut harness, DialogWindow::Connections);
    let _ = harness.app.open_login_scripts();
    assert!(harness.app.login_scripts.is_some());
    let editor = open_window(&mut harness, DialogWindow::LoginScripts);
    focus(&mut harness, main, manager).await;
    assert!(!harness.app.login_scripts_have_keys());
    harness.press("Escape").await;
    assert!(harness.app.connections.is_none(), "the manager closes");
    assert!(harness.app.login_scripts.is_some(), "the editor stays");

    focus(&mut harness, manager, editor).await;
    assert!(harness.app.dialog_open());
    harness.press("F10").await;
    assert!(harness.app.login_scripts.is_none(), "F10 in its window");
    assert!(!harness.app.quit.leaving);
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

/// A copy's conflict question as a card - `appearance.dialog_windows` off - appears while the
/// queue's window has the keyboard: the main window, where the question is drawn, comes to the
/// front - once; there Escape answers it. In its window of its own the question comes to the
/// front itself (`file_ops_tests.rs`).
#[tokio::test]
async fn a_question_brings_the_main_window_to_the_front() {
    let (root, mut harness, main) = with_windows().await;
    harness.send(Message::DialogWindows(false)).await;
    let out = root.path().join("out");
    std::fs::create_dir(&out).expect("mkdir");
    std::fs::write(out.join("a.txt"), b"old").expect("write");
    let navigate = Command::Navigate {
        panel: Some(PanelId::Right),
        path: "out".into(),
    };
    run(&mut harness, navigate).await;
    let cursor = Command::SetCursor {
        panel: Some(PanelId::Left),
        target: CursorTarget::Name("a.txt".into()),
    };
    run(&mut harness, cursor).await;
    harness.send(Message::Queue(QueueMsg::Own(true))).await;
    harness
        .send(Message::Engine(EngineEvent::QueueWindow))
        .await;
    let queue = open_window(&mut harness, DialogWindow::Queue);
    focus(&mut harness, main, queue).await;
    assert_eq!(harness.app.front_for_question(), None, "nothing asked");

    let copy = EngineEvent::FileDialog {
        panel: PanelId::Left,
        dialog: FileDialog::Copy,
    };
    harness.send(Message::Engine(copy)).await;
    let start = FileOpsMsg::Start { background: false };
    harness.send(Message::FileOps(start)).await;
    let job = harness
        .app
        .engine()
        .ops()
        .jobs()
        .last()
        .map(|job| job.id)
        .expect("a job");
    asked(&mut harness, job).await;
    assert_eq!(harness.app.question_window(), None, "a card");
    assert_eq!(harness.app.front_for_question(), Some(main), "to the front");
    assert_eq!(harness.app.front_for_question(), None, "once");

    focus(&mut harness, queue, main).await;
    harness.press("Escape").await;
    tokio::time::timeout(
        Duration::from_secs(10),
        harness.app.engine().ops().wait(job),
    )
    .await
    .expect("in time")
    .expect("known");
    harness.settle().await;
    assert!(
        harness.app.file_ops.jobs.asking().is_none(),
        "answered there"
    );
}
