//! The dialog windows through `apply`: what a window's close request and its moves do. The
//! windows themselves need a display; their books are `book_tests.rs`.

use iced::{Point, window};

use super::{DialogWindow, DialogWindows};
use crate::app::{
    Message, harness::Harness, settings::SettingsMessage, window_state::WindowChange,
};

async fn harness() -> (tempfile::TempDir, Harness, window::Id) {
    let (root, mut harness) = Harness::temp().await;
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    (root, harness, main)
}

/// The settings open, and their window as `sync_dialog_windows` books it.
async fn settings_window(harness: &mut Harness) -> window::Id {
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    assert!(harness.app.settings.is_some());
    let books = &mut harness.app.dialog_windows;
    let (opening, closing) = books.difference(&[DialogWindow::Settings]);
    assert_eq!(opening, [DialogWindow::Settings]);
    assert!(closing.is_empty());
    let id = window::Id::unique();
    let geometry = books.geometry(DialogWindow::Settings, harness.app.window.geometry());
    books.opened(DialogWindow::Settings, id, geometry);
    id
}

/// The title bar's X and Alt+F4 close the settings as Escape does; the window follows the state.
#[tokio::test]
async fn closing_the_settings_window_closes_the_settings() {
    let (_root, mut harness, _main) = harness().await;
    let id = settings_window(&mut harness).await;
    harness.send(Message::CloseRequested(id)).await;
    assert!(harness.app.settings.is_none());
    let (opening, closing) = harness.app.dialog_windows.difference(&[]);
    assert!(opening.is_empty());
    assert_eq!(closing, [id], "its window closes next");
}

/// A close request the dialog's window sent before it closed ends nothing; the main window's
/// quits as F10 does (`app.quit`, RC-080-17): with no job running, it ends the app after storing.
#[tokio::test]
async fn only_the_main_windows_close_request_ends_the_app() {
    let (_root, mut harness, main) = harness().await;
    let id = settings_window(&mut harness).await;
    harness.app.settings = None;
    assert!(harness.app.dialog_windows.closed(id).is_some());
    assert!(harness.app.apply(Message::CloseRequested(id)).is_empty());
    assert_eq!(harness.app.apply(Message::CloseRequested(main)).len(), 1);
    harness.send(Message::CloseRequested(main)).await;
    harness.settle().await;
    assert!(harness.app.quit.leaving, "on the way out");
}

/// A dialog window's moves are its own: the main window's place stays.
#[tokio::test]
async fn a_dialog_windows_moves_leave_the_main_windows_place() {
    let (_root, mut harness, main) = harness().await;
    let id = settings_window(&mut harness).await;
    let before = harness.app.window.geometry();
    let moved = WindowChange::Moved(Point::new(1800.0, 300.0));
    harness.send(Message::WindowSeen(id, moved, false)).await;
    assert_eq!(harness.app.window.geometry(), before);
    let geometries = harness.app.dialog_windows.open_geometries();
    let [(_, dialog)] = geometries.as_slice() else {
        panic!("one dialog window");
    };
    assert_eq!(dialog.position, Some((1800.0, 300.0)));

    harness.send(Message::WindowSeen(main, moved, false)).await;
    assert_eq!(
        harness.app.window.geometry().position,
        Some((1800.0, 300.0))
    );
}
