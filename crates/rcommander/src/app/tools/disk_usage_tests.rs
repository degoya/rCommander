//! The disk-usage window through `apply`, against a real engine: Ctrl+Shift+L opens it for the
//! panel's folder, the walk's tree fills its rows, Enter and Backspace move through the folders,
//! Ctrl+Enter shows a folder in the panel, the largest files are fed to the panel, Escape stops a
//! running walk and closes a finished one. The iced tasks that start the walk and tick do not run
//! in the harness; the tests start the walk and hand the window what the task would.

use std::time::Duration;

use rc_engine::{
    FlatSource, PanelId, VfsPath,
    disk_usage::{DiskUsageHandle, UsageChoices},
};
use tempfile::TempDir;

use super::{Stand, UsageMsg, UsageTab};
use crate::app::{
    Message,
    harness::{self, Harness},
    tools::ToolsMsg,
};

/// `big/` (300 bytes in `big/inner/deep.bin`, 100 in `big/file.bin`), `small/s.bin` (2 bytes)
/// and `top.bin` (1 byte).
fn tree() -> TempDir {
    let (deep, file) = ("x".repeat(300), "x".repeat(100));
    harness::tree(&[
        ("big/inner/deep.bin", deep.as_str()),
        ("big/file.bin", file.as_str()),
        ("small/s.bin", "xx"),
        ("top.bin", "x"),
    ])
}

async fn send(harness: &mut Harness, message: UsageMsg) {
    harness.send(Message::Tools(ToolsMsg::Usage(message))).await;
}

/// Ctrl+Shift+L, then the walk the window's task starts, through to its end.
async fn opened(harness: &mut Harness) -> DiskUsageHandle {
    harness.press("Ctrl+Shift+L").await;
    harness.settle().await;
    let window = harness.app.tools.usage.as_ref().expect("the window");
    assert_eq!(window.stand, Stand::Starting);
    let handle = harness
        .app
        .engine()
        .start_disk_usage(Some(PanelId::Left), None, 10)
        .expect("start");
    for _ in 0..500 {
        if handle.finished() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let generation = window.generation;
    let started = Ok((handle.clone(), UsageChoices::default()));
    send(harness, UsageMsg::Started(generation, started)).await;
    handle
}

fn names(harness: &Harness) -> Vec<String> {
    let window = harness.app.tools.usage.as_ref().expect("open");
    window.rows.iter().map(|row| row.name.clone()).collect()
}

#[tokio::test]
async fn ctrl_shift_l_lists_the_folders_largest_first() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    opened(&mut harness).await;
    let window = harness.app.tools.usage.as_ref().expect("open");
    assert_eq!(window.stand, Stand::Done);
    assert_eq!(window.root, VfsPath::local(root.path()));
    assert_eq!(window.total.bytes, 403);
    assert_eq!(names(&harness), ["big", "small", ""], "own files last");
    assert_eq!(window.rows[0].share, 992, "400 of 403 bytes");
}

#[tokio::test]
async fn enter_backspace_and_ctrl_enter_move_through_the_folders() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    opened(&mut harness).await;
    harness.press("Enter").await;
    assert_eq!(names(&harness), ["inner", ""]);
    harness.press("Ctrl+Enter").await;
    let panel = harness.app.engine().panel(PanelId::Left);
    assert_eq!(panel.path, VfsPath::local(root.path().join("big")));
    assert_eq!(
        panel.cursor_entry().map(|entry| entry.name.as_str()),
        Some("inner")
    );
    harness.press("Backspace").await;
    let window = harness.app.tools.usage.as_ref().expect("open");
    assert_eq!(window.relative, "");
    assert_eq!(
        window.current().map(|row| row.name.as_str()),
        Some("big"),
        "the cursor on the folder left"
    );
}

#[tokio::test]
async fn the_largest_files_feed_the_panel() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    opened(&mut harness).await;
    send(&mut harness, UsageMsg::Tab(UsageTab::Largest)).await;
    let window = harness.app.tools.usage.as_ref().expect("open");
    let sizes: Vec<u64> = window.largest.iter().map(|file| file.bytes).collect();
    assert_eq!(sizes, [300, 100, 2, 1]);
    send(&mut harness, UsageMsg::Feed).await;
    let mut panel = harness.app.engine().panel(PanelId::Left);
    for _ in 0..500 {
        if panel.flat.as_ref().is_some_and(|flat| !flat.loading) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        panel = harness.app.engine().panel(PanelId::Left);
    }
    assert_eq!(panel.flat.expect("flat").source, FlatSource::Paths);
    assert_eq!(panel.entries.len(), 4);
}

#[tokio::test]
async fn escape_stops_a_running_walk_and_closes_a_finished_one() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Shift+L").await;
    harness.settle().await;
    harness.press("Escape").await;
    assert!(
        harness.app.tools.usage.is_some(),
        "Escape while the walk runs stops it; the window stays"
    );
    send(&mut harness, UsageMsg::Close).await;
    assert!(harness.app.tools.usage.is_none());
    opened(&mut harness).await;
    harness.press("Ctrl+R").await;
    let window = harness.app.tools.usage.as_ref().expect("open");
    assert_eq!(window.stand, Stand::Starting, "Ctrl+R reads it again");
    send(&mut harness, UsageMsg::Close).await;
    opened(&mut harness).await;
    harness.press("Escape").await;
    assert!(harness.app.tools.usage.is_none());
}

/// RC-110-29: a walk that starts after its window closed is stopped, and the window opened since
/// does not take it for its own.
#[tokio::test]
async fn a_walk_started_for_a_closed_window_is_stopped() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Shift+L").await;
    harness.settle().await;
    let first = harness.app.tools.usage.as_ref().expect("open").generation;
    send(&mut harness, UsageMsg::Close).await;
    let engine = harness.app.engine().clone();
    let late = engine
        .start_disk_usage(Some(PanelId::Left), None, 10)
        .expect("start");
    let started = Ok((late.clone(), UsageChoices::default()));
    send(&mut harness, UsageMsg::Started(first, started)).await;
    assert!(late.stop_asked(), "no window: the walk stops");

    harness.press("Ctrl+Shift+L").await;
    harness.settle().await;
    let window = harness.app.tools.usage.as_ref().expect("open again");
    assert_ne!(window.generation, first, "a number no earlier window had");
    let other = engine
        .start_disk_usage(Some(PanelId::Left), None, 10)
        .expect("start");
    let started = Ok((other.clone(), UsageChoices::default()));
    send(&mut harness, UsageMsg::Started(first, started)).await;
    let window = harness.app.tools.usage.as_ref().expect("open again");
    assert!(other.stop_asked(), "the earlier window's walk stops");
    assert_eq!(window.stand, Stand::Starting, "and is not this window's");
}

/// RC-110-29: a tick reads only the tab shown, and builds its list only when the sum moved.
#[tokio::test]
async fn a_tick_reads_the_tab_shown_and_only_a_changed_sum() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    opened(&mut harness).await;
    let window = harness.app.tools.usage.as_mut().expect("open");
    let generation = window.generation;
    assert!(window.largest.is_empty(), "the tab not shown is not read");
    assert_eq!(window.rows.len(), 3);
    // The walk is through: the sum cannot move, so a tick leaves the rows as they are.
    window.rows.clear();
    send(&mut harness, UsageMsg::Tick(generation)).await;
    let window = harness.app.tools.usage.as_ref().expect("open");
    assert!(window.rows.is_empty(), "the same sum builds no rows");
    send(&mut harness, UsageMsg::Tab(UsageTab::Largest)).await;
    let window = harness.app.tools.usage.as_ref().expect("open");
    assert_eq!(window.largest.len(), 4, "read when its tab is shown");
}
