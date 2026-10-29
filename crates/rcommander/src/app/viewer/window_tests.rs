//! Lister in windows of its own through `apply` (RC-080-15): one window per Lister, each taking
//! the keys only while it has the keyboard, Escape, F10 and the X closing that Lister alone, the
//! panels keeping their keys beside them; with the setting off the Listers lie over the main
//! window as before. The windows themselves need a display.

use iced::window;
use rc_engine::{EngineEvent, PanelId, VfsPath};

use crate::app::{
    Message,
    dialog_windows::{DialogWindow, DialogWindows},
    harness::Harness,
    queue_window::QueueMsg,
};

/// The app over a folder with `a.txt` and `b.txt`, `appearance.dialog_windows` on and its main
/// window `main`.
async fn with_windows() -> (tempfile::TempDir, Harness, window::Id) {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::write(root.path().join("a.txt"), b"a").expect("write");
    std::fs::write(root.path().join("b.txt"), b"b").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    let main = window::Id::unique();
    harness.app.dialog_windows = DialogWindows::new(main, Vec::new());
    harness.send(Message::DialogWindows(true)).await;
    (root, harness, main)
}

/// F3 on `name`: a Lister for it, and its number.
async fn view(harness: &mut Harness, root: &tempfile::TempDir, name: &str) -> u64 {
    let path = VfsPath::local(root.path().join(name));
    let event = EngineEvent::ViewFile {
        panel: PanelId::Left,
        path,
    };
    harness.send(Message::Engine(event)).await;
    harness
        .app
        .viewers
        .open
        .last()
        .map(|lister| lister.id)
        .expect("a Lister")
}

/// The window `sync_dialog_windows` would open for the Lister `id`, booked: its id.
fn open_window(harness: &mut Harness, id: u64) -> window::Id {
    let kind = DialogWindow::Lister(id);
    let wanted = harness.app.wanted_windows();
    let books = &mut harness.app.dialog_windows;
    let (opening, _) = books.difference(&wanted);
    assert_eq!(opening, [kind], "a window for {kind:?}");
    let window = window::Id::unique();
    let geometry = books.geometry(kind, harness.app.window.geometry());
    books.opened(kind, window, geometry);
    window
}

async fn focus(harness: &mut Harness, from: window::Id, to: window::Id) {
    harness
        .send(Message::Queue(QueueMsg::Focused(from, false)))
        .await;
    harness
        .send(Message::Queue(QueueMsg::Focused(to, true)))
        .await;
}

fn listers(harness: &Harness) -> Vec<u64> {
    harness
        .app
        .viewers
        .open
        .iter()
        .map(|lister| lister.id)
        .collect()
}

/// Each Lister has a window of its own; Escape in one closes that Lister, not the newest, and
/// its window closes next.
#[tokio::test]
async fn each_lister_has_a_window_and_escape_closes_only_its_own() {
    let (root, mut harness, main) = with_windows().await;
    let first = view(&mut harness, &root, "a.txt").await;
    let first_window = open_window(&mut harness, first);
    let second = view(&mut harness, &root, "b.txt").await;
    let second_window = open_window(&mut harness, second);
    assert_eq!(
        harness.app.wanted_windows(),
        [DialogWindow::Lister(first), DialogWindow::Lister(second)]
    );
    assert!(harness.app.lister_title(first).contains("a.txt"));

    focus(&mut harness, main, first_window).await;
    harness.press("Escape").await;
    assert_eq!(listers(&harness), [second], "the older Lister closed");
    let (_, closing) = harness
        .app
        .dialog_windows
        .difference(&harness.app.wanted_windows());
    assert_eq!(closing, [first_window]);
    assert!(harness.app.dialog_windows.closed(first_window).is_some());

    // F10 in a Lister's window closes it and quits nothing.
    focus(&mut harness, first_window, second_window).await;
    harness.press("F10").await;
    assert!(listers(&harness).is_empty());
    assert!(!harness.app.quit.leaving);
}

/// With the main window focused the panels keep their keys beside a Lister's window - F7 opens
/// the new-folder dialog, Escape closes that - and the X of the Lister's window closes it.
#[tokio::test]
async fn the_panels_keep_their_keys_beside_a_lister_window() {
    let (root, mut harness, main) = with_windows().await;
    let id = view(&mut harness, &root, "a.txt").await;
    let lister_window = open_window(&mut harness, id);
    focus(&mut harness, lister_window, main).await;
    assert!(
        !harness.app.dialog_open(),
        "no Lister holds the main window"
    );

    harness.press("F7").await;
    assert!(harness.app.file_ops.dialog.is_some(), "a panel key");
    harness.press("Escape").await;
    assert!(harness.app.file_ops.dialog.is_none());
    assert_eq!(listers(&harness), [id], "the Lister stays open");

    harness.send(Message::CloseRequested(lister_window)).await;
    assert!(listers(&harness).is_empty(), "its X closes it");
    assert!(!harness.app.quit.leaving);
}

/// Every Lister opens where the last one is or was: they share one place.
#[tokio::test]
async fn the_listers_share_one_place() {
    let (root, mut harness, _main) = with_windows().await;
    let first = view(&mut harness, &root, "a.txt").await;
    let first_window = open_window(&mut harness, first);
    let moved = crate::app::window_state::WindowChange::Moved(iced::Point::new(1500.0, 120.0));
    harness
        .send(Message::WindowSeen(first_window, moved, false))
        .await;
    let second = view(&mut harness, &root, "b.txt").await;
    let main_place = harness.app.window.geometry();
    let books = &harness.app.dialog_windows;
    let place = books.geometry(DialogWindow::Lister(second), main_place);
    assert_eq!(
        place.position,
        Some((1500.0, 120.0)),
        "where the open one is"
    );
    assert_eq!(DialogWindow::Lister(first).key(), "window.dialog.lister");
}

/// With the setting off the Listers lie over the main window and the top one has its keys, as
/// before: F7 is Lister's search there, not the panel's new folder.
#[tokio::test]
async fn off_the_listers_lie_over_the_main_window() {
    let (root, mut harness, _main) = with_windows().await;
    harness.send(Message::DialogWindows(false)).await;
    view(&mut harness, &root, "a.txt").await;
    assert!(harness.app.wanted_windows().is_empty());
    assert!(harness.app.lister_over_main());
    harness.press("F7").await;
    assert!(harness.app.file_ops.dialog.is_none(), "the Lister's key");
}
