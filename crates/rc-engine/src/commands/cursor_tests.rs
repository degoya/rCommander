use tempfile::TempDir;

use super::stepped;
use crate::test_support;
use crate::test_support::run_panel;
use crate::{Command, CursorStep, EngineEvent, PanelChange, PanelId, PanelSnapshot};

/// `root/` with the directories `alpha/` (holding `inner.txt`) and `beta/`, and the files
/// `c.txt` to `f.txt`: six entries in panel order.
fn tree() -> TempDir {
    test_support::tree(&[
        ("alpha/inner.txt", "i"),
        ("beta/", ""),
        ("c.txt", "x"),
        ("d.txt", "x"),
        ("e.txt", "x"),
        ("f.txt", "x"),
    ])
}

fn step(step: CursorStep) -> Command {
    Command::MoveCursor { panel: None, step }
}

fn cursor_name(snapshot: &PanelSnapshot) -> Option<&str> {
    snapshot.cursor.as_ref().map(|cursor| cursor.name.as_str())
}

#[test]
fn a_step_stays_inside_the_entries() {
    assert_eq!(stepped(0, 10, 4, 0, CursorStep::Up), 0);
    assert_eq!(stepped(9, 10, 4, 0, CursorStep::Down), 9);
    assert_eq!(stepped(2, 10, 4, 0, CursorStep::PageUp), 0);
    assert_eq!(stepped(7, 10, 4, 0, CursorStep::PageDown), 9);
    assert_eq!(stepped(5, 10, 4, 0, CursorStep::PageDown), 9);
    assert_eq!(stepped(5, 10, 4, 0, CursorStep::First), 0);
    assert_eq!(stepped(0, 10, 4, 0, CursorStep::Last), 9);
    for every in [CursorStep::Up, CursorStep::Down, CursorStep::Last] {
        assert_eq!(
            stepped(0, 0, 4, 0, every),
            0,
            "{every:?} in an empty directory"
        );
    }
}

#[tokio::test]
async fn the_cursor_pages_by_the_rows_the_window_reported() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    engine.set_page_rows(PanelId::Left, 3);
    let paged = run_panel(&engine, step(CursorStep::PageDown)).await;
    assert_eq!(paged.cursor.as_ref().map(|cursor| cursor.index), Some(2));
    assert_eq!(
        events.try_recv().expect("an event"),
        EngineEvent::Panel {
            panel: PanelId::Left,
            change: PanelChange::Cursor
        }
    );
    let last = run_panel(&engine, step(CursorStep::Last)).await;
    assert_eq!(cursor_name(&last), Some("f.txt"));
    let up = run_panel(&engine, step(CursorStep::Up)).await;
    assert_eq!(cursor_name(&up), Some("e.txt"));
    assert_eq!(
        engine.panel(PanelId::Right).cursor,
        0,
        "the other panel stays"
    );
}

#[tokio::test]
async fn open_entry_enters_a_directory_and_leaves_a_file() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let inside = run_panel(&engine, Command::OpenEntry { panel: None }).await;
    assert_eq!(inside.path.path, root.path().join("alpha"));
    assert_eq!(cursor_name(&inside), Some("inner.txt"));

    let back = run_panel(
        &engine,
        Command::Navigate {
            panel: None,
            path: "..".into(),
        },
    )
    .await;
    assert_eq!(cursor_name(&back), Some("alpha"));
    run_panel(&engine, step(CursorStep::Last)).await;
    let mut events = engine.subscribe();
    let stays = run_panel(&engine, Command::OpenEntry { panel: None }).await;
    assert_eq!(stays.path.path, root.path());
    assert_eq!(cursor_name(&stays), Some("f.txt"));
    assert!(events.try_recv().is_err(), "a file announces nothing");
}

#[tokio::test]
async fn insert_flips_the_cursor_entry_and_moves_down() {
    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    let once = run_panel(&engine, Command::ToggleCursorSelection { panel: None }).await;
    assert_eq!(once.selection, ["alpha"]);
    assert_eq!(cursor_name(&once), Some("beta"));
    assert_eq!(
        events.try_recv().expect("selection"),
        EngineEvent::Panel {
            panel: PanelId::Left,
            change: PanelChange::Selection
        }
    );
    run_panel(&engine, step(CursorStep::Up)).await;
    let twice = run_panel(&engine, Command::ToggleCursorSelection { panel: None }).await;
    assert!(twice.selection.is_empty());
}

/// The toggle starts the default shell (`terminal_view_tests` has the rest).
#[cfg(unix)]
#[tokio::test]
async fn the_terminal_view_toggles_and_survives_navigation() {
    use crate::PanelView;

    let root = tree();
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    let shown = run_panel(
        &engine,
        Command::ToggleTerminal {
            panel: Some(PanelId::Right),
        },
    )
    .await;
    assert_eq!(shown.view, PanelView::Terminal);
    let view = EngineEvent::Panel {
        panel: PanelId::Right,
        change: PanelChange::View,
    };
    let mut announced = Vec::new();
    while let Ok(event) = events.try_recv() {
        announced.push(event);
    }
    assert!(announced.contains(&view), "{announced:?}");
    let moved = run_panel(
        &engine,
        Command::Navigate {
            panel: Some(PanelId::Right),
            path: "beta".into(),
        },
    )
    .await;
    assert_eq!(moved.view, PanelView::Terminal);
    assert_eq!(engine.panel(PanelId::Left).view, PanelView::Files);
    let back = run_panel(
        &engine,
        Command::ToggleTerminal {
            panel: Some(PanelId::Right),
        },
    )
    .await;
    assert_eq!(back.view, PanelView::Files);
}

#[test]
fn a_column_step_moves_by_a_column_in_the_brief_view_only() {
    assert_eq!(stepped(2, 30, 9, 10, CursorStep::Right), 12);
    assert_eq!(stepped(12, 30, 9, 10, CursorStep::Left), 2);
    assert_eq!(stepped(25, 30, 9, 10, CursorStep::Right), 29);
    assert_eq!(stepped(4, 30, 9, 10, CursorStep::Left), 0);
    assert_eq!(stepped(5, 30, 9, 0, CursorStep::Left), 5, "the full view");
    assert_eq!(stepped(5, 30, 9, 0, CursorStep::Right), 5, "the full view");
}
