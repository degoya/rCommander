//! The window's logic through `apply`, without a window: keys and clicks become commands, the
//! engine runs them, and its events bring the panels up to date.

use std::{fs, path::Path};

use iced::{Size, mouse::ScrollDelta};
use rc_engine::{Command, PanelId};
use tempfile::TempDir;

use super::{
    Message,
    harness::{self, Harness},
};
use crate::theme::ROW_HEIGHT;

/// `root/` with the directories `inner/` (holding `deeper/` and `file.txt`) and `outer/`, then
/// the files `f00.txt` to `f39.txt`: 42 entries, directories first.
pub(super) fn tree() -> TempDir {
    let numbered: Vec<String> = (0..40).map(|i| format!("f{i:02}.txt")).collect();
    let mut entries = vec![
        ("inner/deeper/", ""),
        ("inner/file.txt", "x"),
        ("outer/", ""),
    ];
    entries.extend(numbered.iter().map(|name| (name.as_str(), "x")));
    harness::tree(&entries)
}

pub(super) fn path(harness: &Harness, panel: PanelId) -> &Path {
    &harness.app.pane(panel).state.path.path
}

pub(super) fn cursor_name(harness: &Harness, panel: PanelId) -> String {
    let pane = harness.app.pane(panel);
    pane.entries()[pane.cursor()].name.clone()
}

#[tokio::test]
async fn both_panels_start_in_the_start_directory() {
    let root = tree();
    let harness = Harness::headless(root.path()).await;
    assert_eq!(path(&harness, PanelId::Left), root.path());
    assert_eq!(path(&harness, PanelId::Right), root.path());
    assert_eq!(harness.app.active, PanelId::Left);
    assert_eq!(harness.app.pane(PanelId::Left).entries().len(), 42);
}

#[tokio::test]
async fn tab_switches_the_active_panel_in_the_engine_and_on_screen() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Right);
    assert_eq!(harness.app.engine().active_panel(), PanelId::Right);
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Left);
}

#[tokio::test]
async fn keys_act_on_the_active_panel_in_the_order_pressed() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    // The cursor starts on `..`: down onto `inner`, `outer`, `f00.txt`, and up again.
    for keys in ["Tab", "ArrowDown", "ArrowDown", "ArrowDown", "ArrowUp"] {
        harness.press(keys).await;
    }
    assert_eq!(cursor_name(&harness, PanelId::Right), "outer");
    assert!(harness.app.pane(PanelId::Left).on_parent());
}

#[tokio::test]
async fn enter_opens_a_directory_and_backspace_returns_onto_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("ArrowDown").await;
    harness.press("Enter").await;
    assert_eq!(path(&harness, PanelId::Left), root.path().join("inner"));
    assert!(harness.app.pane(PanelId::Left).on_parent());
    let names: Vec<&str> = harness
        .app
        .pane(PanelId::Left)
        .entries()
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, ["deeper", "file.txt"]);
    harness.press("Backspace").await;
    assert_eq!(path(&harness, PanelId::Left), root.path());
    assert_eq!(cursor_name(&harness, PanelId::Left), "inner");
    assert_eq!(
        path(&harness, PanelId::Right),
        root.path(),
        "the other panel stays"
    );
}

#[tokio::test]
async fn insert_selects_and_moves_down_and_the_status_counts_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Insert").await;
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!(
        (pane.cursor_row(), pane.selected_count()),
        (1, 0),
        "`..` is skipped"
    );
    harness.press("Insert").await;
    harness.press("Space").await;
    let pane = harness.app.pane(PanelId::Left);
    assert!(pane.is_selected(1) && pane.is_selected(2));
    assert_eq!((pane.cursor(), pane.selected_count()), (2, 2));
}

#[tokio::test]
async fn page_down_pages_by_the_rows_the_window_measured() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let ten_rows = Size::new(400.0, 10.0 * ROW_HEIGHT);
    harness
        .send(Message::Viewport(PanelId::Left, ten_rows))
        .await;
    assert_eq!(harness.app.pane(PanelId::Left).page_rows(), 10);
    harness.press("PageDown").await;
    assert_eq!(harness.app.pane(PanelId::Left).cursor(), 9);
    harness.press("End").await;
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!((pane.cursor(), pane.cursor_row(), pane.top()), (41, 42, 33));
    assert_eq!(pane.visible_range(), 33..43);
}

#[tokio::test]
async fn the_wheel_scrolls_the_panel_but_not_its_cursor() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let ten_rows = Size::new(400.0, 10.0 * ROW_HEIGHT);
    harness
        .send(Message::Viewport(PanelId::Left, ten_rows))
        .await;
    let wheel = ScrollDelta::Lines { x: 0.0, y: -2.0 };
    harness.send(Message::Scroll(PanelId::Left, wheel)).await;
    assert_eq!(harness.app.pane(PanelId::Left).top(), 6);
    assert_eq!(harness.app.pane(PanelId::Left).cursor(), 0);
}

#[tokio::test]
async fn a_click_activates_the_panel_and_a_double_click_opens_the_row() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.send(Message::Click(PanelId::Right, 3)).await;
    assert_eq!(harness.app.active, PanelId::Right);
    assert_eq!(cursor_name(&harness, PanelId::Right), "f00.txt");
    harness.send(Message::Open(PanelId::Left, 2)).await;
    assert_eq!(harness.app.active, PanelId::Left);
    assert_eq!(path(&harness, PanelId::Left), root.path().join("outer"));
    harness.send(Message::Open(PanelId::Left, 0)).await;
    assert_eq!(path(&harness, PanelId::Left), root.path(), "`..` goes up");
    assert_eq!(cursor_name(&harness, PanelId::Left), "outer");
}

#[tokio::test]
async fn a_press_over_the_other_panel_activates_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.send(Message::Hover(PanelId::Right)).await;
    harness.send(Message::Press).await;
    assert_eq!(harness.app.active, PanelId::Right);
    harness.send(Message::Unhover(PanelId::Right)).await;
    harness.send(Message::Press).await;
    assert_eq!(harness.app.active, PanelId::Right, "a press over nothing");
}

#[tokio::test]
async fn a_failed_command_shows_its_message_until_the_panel_moves() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    fs::remove_dir_all(root.path().join("inner")).expect("rm");
    harness.press("ArrowDown").await;
    harness.press("Enter").await;
    assert_eq!(path(&harness, PanelId::Left), root.path());
    assert_eq!(
        harness.app.pane(PanelId::Left).error.as_deref(),
        Some("Not found.")
    );
    harness.press("ArrowDown").await;
    harness.press("Enter").await;
    assert_eq!(path(&harness, PanelId::Left), root.path().join("outer"));
    assert_eq!(harness.app.pane(PanelId::Left).error, None);
}

/// What an MCP client does reaches the engine without the window; the window catches up.
#[tokio::test]
async fn a_change_from_outside_the_window_shows_in_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let outside = harness.app.engine().clone();
    outside
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "inner".into(),
        })
        .await
        .expect("navigates");
    outside
        .dispatch(Command::SwitchActivePanel { panel: None })
        .await
        .expect("switches");
    harness.settle().await;
    assert_eq!(path(&harness, PanelId::Right), root.path().join("inner"));
    assert_eq!(harness.app.active, PanelId::Right);
}

#[tokio::test]
async fn missed_events_read_every_panel_again() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness
        .app
        .engine()
        .clone()
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: "outer".into(),
        })
        .await
        .expect("navigates");
    harness.miss_events();
    assert_eq!(path(&harness, PanelId::Left), root.path());
    harness.app.pane_mut(PanelId::Left).error = Some("Not found.".to_owned());
    harness.send(Message::EngineLagged).await;
    assert_eq!(path(&harness, PanelId::Left), root.path().join("outer"));
    // RC-080-53: the panel moved while the window missed it - the old folder's message goes.
    assert_eq!(harness.app.pane(PanelId::Left).error, None);
}

#[tokio::test]
async fn a_key_the_keymap_does_not_know_does_nothing() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    for keys in ["F9", "Ctrl+Shift+F", "Alt+X"] {
        harness.press(keys).await;
    }
    assert_eq!(harness.app.pane(PanelId::Left).cursor(), 0);
    assert_eq!(harness.app.active, PanelId::Left);
}
