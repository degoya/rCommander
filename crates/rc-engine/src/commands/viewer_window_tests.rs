use tempfile::TempDir;

use crate::test_support;
use crate::{Command, CursorTarget, EngineEvent, PanelId, VfsPath};

fn tree() -> TempDir {
    test_support::tree(&[("sub/", ""), ("notes.txt", "hello\n")])
}

#[tokio::test]
async fn f3_announces_the_file_under_the_cursor_for_its_panel() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    engine
        .dispatch(Command::SetCursor {
            panel: Some(PanelId::Right),
            target: CursorTarget::Name("notes.txt".into()),
        })
        .await
        .expect("cursor");
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ViewFile {
            panel: Some(PanelId::Right),
            path: None,
        })
        .await
        .expect("announces");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::ViewFile {
            panel: PanelId::Right,
            path: VfsPath::local(root.path().join("notes.txt")),
        })
    );
}

#[tokio::test]
async fn a_named_file_is_resolved_against_the_panel_and_opens_for_viewing() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ViewFile {
            panel: None,
            path: Some("notes.txt".into()),
        })
        .await
        .expect("announces");
    let Ok(EngineEvent::ViewFile { path, .. }) = events.try_recv() else {
        panic!("a view event");
    };
    let window = engine.open_for_view(&path).await.expect("opens");
    assert_eq!(window.size(), 6);
}

#[tokio::test]
async fn a_directory_is_refused_and_nothing_is_announced() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    let error = engine
        .dispatch(Command::ViewFile {
            panel: None,
            path: Some("sub".into()),
        })
        .await
        .expect_err("a directory");
    assert_eq!(error.code(), "is_a_directory");
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn ctrl_q_announces_the_quick_view_of_the_panel() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::ToggleQuickView {
            panel: Some(PanelId::Left),
        })
        .await
        .expect("announces");
    assert_eq!(
        events.try_recv().ok(),
        Some(EngineEvent::QuickView {
            panel: PanelId::Left
        })
    );
}

#[test]
fn both_read_from_the_keymap_spelling() {
    let view: Command =
        serde_json::from_str(r#"{"command":"view.open","params":{}}"#).expect("view.open");
    assert_eq!(
        view,
        Command::ViewFile {
            panel: None,
            path: None
        }
    );
    let quick: Command =
        serde_json::from_str(r#"{"command":"panel.toggle_quick_view","params":{}}"#)
            .expect("quick view");
    assert_eq!(quick, Command::ToggleQuickView { panel: None });
}
