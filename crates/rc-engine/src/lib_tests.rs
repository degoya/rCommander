use tempfile::TempDir;

use super::*;
use crate::test_support;
use crate::test_support::expect_panel;

/// `root/` with `beta/`, `Alpha/` (directories) and `zeta.txt`, `b.txt` (files).
fn tree() -> TempDir {
    test_support::tree(&[
        ("beta/", ""),
        ("Alpha/", ""),
        ("zeta.txt", "z"),
        ("b.txt", "bb"),
        ("beta/inner.txt", "i"),
    ])
}

fn names(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

#[tokio::test]
async fn panels_list_directories_first_then_files_ignoring_case() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let left = engine.panel(PanelId::Left);
    assert_eq!(names(&left.entries), ["Alpha", "beta", "b.txt", "zeta.txt"]);
    assert_eq!(engine.active_panel(), PanelId::Left);
}

#[tokio::test]
async fn list_dir_moves_no_panel() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    let outcome = engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some("beta".into()),
        })
        .await
        .expect("lists");
    let Outcome::Listing { entries, .. } = outcome else {
        panic!("a listing");
    };
    assert_eq!(names(&entries), ["inner.txt"]);
    assert_eq!(engine.panel(PanelId::Left).path.path, root.path());
    assert!(events.try_recv().is_err(), "a read announces nothing");
}

#[tokio::test]
async fn navigate_down_and_up_puts_the_cursor_on_the_directory_left() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();

    let down = expect_panel(
        engine
            .dispatch(Command::Navigate {
                panel: None,
                path: "beta".into(),
            })
            .await
            .expect("navigates"),
    );
    assert_eq!(down.path.path, root.path().join("beta"));
    assert_eq!(down.entry_count, 1);
    assert_eq!(
        events.recv().await.expect("event"),
        EngineEvent::Panel {
            panel: PanelId::Left,
            change: PanelChange::Navigated
        }
    );

    let up = expect_panel(
        engine
            .dispatch(Command::Navigate {
                panel: Some(PanelId::Left),
                path: "..".into(),
            })
            .await
            .expect("navigates"),
    );
    assert_eq!(up.path.path, root.path());
    assert_eq!(up.cursor.expect("cursor").name, "beta");
    assert_eq!(engine.panel(PanelId::Right).path.path, root.path());
}

#[tokio::test]
async fn navigate_to_a_missing_or_file_path_keeps_the_panel_and_names_the_code() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let missing = engine
        .dispatch(Command::Navigate {
            panel: None,
            path: "nope".into(),
        })
        .await
        .expect_err("refused");
    assert_eq!(missing.code(), "not_found");
    let file = engine
        .dispatch(Command::Navigate {
            panel: None,
            path: "b.txt".into(),
        })
        .await
        .expect_err("refused");
    assert_eq!(file.code(), "not_a_directory");
    assert_eq!(engine.panel(PanelId::Left).path.path, root.path());
}

#[tokio::test]
async fn cursor_by_index_and_name_and_out_of_range() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let by_name = expect_panel(
        engine
            .dispatch(Command::SetCursor {
                panel: None,
                target: CursorTarget::Name("zeta.txt".into()),
            })
            .await
            .expect("moves"),
    );
    assert_eq!(by_name.cursor.expect("cursor").index, 3);
    let by_index = expect_panel(
        engine
            .dispatch(Command::SetCursor {
                panel: None,
                target: CursorTarget::Index(1),
            })
            .await
            .expect("moves"),
    );
    assert_eq!(by_index.cursor.expect("cursor").name, "beta");
    let error = engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: CursorTarget::Index(4),
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "cursor_out_of_range");
}

#[tokio::test]
async fn selection_toggles_and_an_unknown_name_changes_nothing() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let selected = expect_panel(
        engine
            .dispatch(Command::ToggleSelection {
                panel: Some(PanelId::Right),
                names: vec!["b.txt".into(), "Alpha".into()],
            })
            .await
            .expect("toggles"),
    );
    assert_eq!(selected.selection, ["Alpha", "b.txt"]);
    let error = engine
        .dispatch(Command::ToggleSelection {
            panel: Some(PanelId::Right),
            names: vec!["b.txt".into(), "ghost".into()],
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code(), "entry_not_found");
    let state = expect_panel(
        engine
            .dispatch(Command::ToggleSelection {
                panel: Some(PanelId::Right),
                names: vec!["b.txt".into()],
            })
            .await
            .expect("toggles"),
    );
    assert_eq!(state.selection, ["Alpha"]);
    assert!(engine.panel(PanelId::Left).selection.is_empty());
}

#[tokio::test]
async fn switching_panels_is_shared_by_every_clone() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let other = engine.clone();
    let mut events = other.subscribe();
    let right = expect_panel(
        engine
            .dispatch(Command::SwitchActivePanel { panel: None })
            .await
            .expect("switches"),
    );
    assert_eq!(right.panel, PanelId::Right);
    assert!(right.active);
    assert_eq!(other.active_panel(), PanelId::Right);
    assert_eq!(
        events.recv().await.expect("event"),
        EngineEvent::ActivePanel {
            panel: PanelId::Right
        }
    );
    let state = expect_panel(
        other
            .dispatch(Command::GetPanelState { panel: None })
            .await
            .expect("reads"),
    );
    assert_eq!(state.panel, PanelId::Right);
    let left = expect_panel(
        other
            .dispatch(Command::GetPanelState {
                panel: Some(PanelId::Left),
            })
            .await
            .expect("reads"),
    );
    assert!(!left.active);
}

#[test]
fn the_engine_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync + Clone + 'static>() {}
    assert_send_sync::<Engine>();
}

#[test]
fn error_codes_are_stable() {
    assert_eq!(EngineError::VaultLocked.code(), "vault_locked");
    assert_eq!(
        EngineError::BackendUnavailable.code(),
        "backend_unavailable"
    );
    assert_eq!(
        EngineError::EntryNotFound { name: "x".into() }.code(),
        "entry_not_found"
    );
}
