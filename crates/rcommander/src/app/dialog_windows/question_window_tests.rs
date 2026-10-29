//! The window of a small question (RC-080-23): owned by the window it would hide behind - the
//! front modal dialog's, the main window, the compare window for the synchronization's
//! confirmation -, closed before that window closes, and opened again owned by the next one.
//! The windows themselves need a display; these are their books.

use std::time::Instant;

use iced::window;
use rc_engine::EngineEvent;

use super::super::questions::Question;
use crate::app::{
    Message,
    dialog_windows::{DialogWindow, DialogWindowMsg, DialogWindows},
    harness::Harness,
    settings::SettingsMessage,
};

/// The app with `appearance.dialog_windows` on and its main window `main`.
async fn app() -> (tempfile::TempDir, Harness, window::Id) {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::headless(root.path()).await;
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    harness.send(Message::DialogWindows(true)).await;
    (root, harness, main)
}

/// What `update` does after a message, with the system's part played: the windows to open and
/// close (`sync_dialog_windows`), and each window made ready and shown in the order it was asked
/// for (`DialogWindowMsg`).
async fn sync(harness: &mut Harness) {
    let _ = harness.app.sync_dialog_windows();
    let since = Instant::now();
    let books = &harness.app.dialog_windows;
    let made: Vec<window::Id> = harness
        .app
        .wanted_windows()
        .into_iter()
        .filter(|kind| books.visible(*kind).is_none())
        .filter_map(|kind| books.id_of(kind))
        .collect();
    for id in made {
        let reused = false;
        for message in [
            DialogWindowMsg::Ready { id, since, reused },
            DialogWindowMsg::Shown { id, since, reused },
        ] {
            harness.send(Message::DialogWindow(message)).await;
        }
    }
}

async fn ask_quit(harness: &mut Harness) {
    harness
        .send(Message::Engine(EngineEvent::Quit { unfinished: 1 }))
        .await;
}

fn quit_window(harness: &Harness) -> Option<window::Id> {
    harness
        .app
        .dialog_windows
        .id_of(DialogWindow::Question(Question::Quit))
}

/// Without a modal dialog the main window owns the question's window and is locked by it; the
/// lock goes with the question's window.
#[tokio::test]
async fn without_a_modal_dialog_the_main_window_owns_the_question() {
    let (_root, mut harness, main) = app().await;
    ask_quit(&mut harness).await;
    sync(&mut harness).await;
    let id = quit_window(&harness).expect("the question's window");
    assert_eq!(harness.app.dialog_windows.question_owner, Some((id, main)));
    assert!(
        harness.app.dialog_windows.modal_open(),
        "the main window locked"
    );

    harness.press("Escape").await;
    sync(&mut harness).await;
    assert_eq!(quit_window(&harness), None, "closed");
    assert_eq!(harness.app.dialog_windows.question_owner, None);
    assert!(
        !harness.app.dialog_windows.modal_open(),
        "and the lock with it"
    );
}

/// The modal dialog's window that owns the question's window closes: the question's window
/// closes first - Windows would destroy it with its owner - and the question, still asked,
/// opens again owned by the main window.
#[tokio::test]
async fn the_owner_closing_closes_the_question_first_and_it_opens_again() {
    let (_root, mut harness, main) = app().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    ask_quit(&mut harness).await;
    sync(&mut harness).await;
    let settings = harness
        .app
        .dialog_windows
        .id_of(DialogWindow::Settings)
        .expect("the settings' window");
    let id = quit_window(&harness).expect("the question's window");
    assert_eq!(
        harness.app.dialog_windows.question_owner,
        Some((id, settings))
    );

    let _ = harness.app.close_dialog_window(settings);
    let books = &harness.app.dialog_windows;
    assert_eq!(books.id_of(DialogWindow::Settings), None);
    assert_eq!(
        quit_window(&harness),
        None,
        "the question's window went first"
    );
    assert_eq!(harness.app.dialog_windows.question_owner, None);

    // The settings are closed with their window; the question still waits.
    harness.app.settings = None;
    sync(&mut harness).await;
    let again = quit_window(&harness).expect("open again");
    assert_ne!(again, id);
    assert_eq!(
        harness.app.dialog_windows.question_owner,
        Some((again, main)),
        "owned by the main window now"
    );
}

/// A modal dialog's window that opens while a question's window is open - a job's question while
/// the MCP call is asked - would lie in front of the question that has the keys: once the
/// dialog's window is shown, the question's closes and opens again in the same round, over the
/// dialog's and owned by it.
#[tokio::test]
async fn a_modal_window_opening_over_a_question_takes_it_on_top() {
    let (_root, mut harness, main) = app().await;
    ask_quit(&mut harness).await;
    sync(&mut harness).await;
    let first = quit_window(&harness).expect("the question's window");
    assert_eq!(
        harness.app.dialog_windows.question_owner,
        Some((first, main))
    );

    harness.send(Message::Settings(SettingsMessage::Open)).await;
    assert!(
        harness.app.settings.is_some(),
        "the settings open beside the question"
    );
    sync(&mut harness).await;
    assert_eq!(
        quit_window(&harness),
        Some(first),
        "until the settings' window shows"
    );
    sync(&mut harness).await;
    let settings = harness
        .app
        .dialog_windows
        .id_of(DialogWindow::Settings)
        .expect("the settings' window");
    let again = quit_window(&harness).expect("the question's window again");
    assert_ne!(again, first, "a new window, over the settings'");
    assert_eq!(
        harness.app.dialog_windows.question_owner,
        Some((again, settings))
    );
    assert_eq!(harness.app.dialog_windows.front_modal(), Some(settings));
}
