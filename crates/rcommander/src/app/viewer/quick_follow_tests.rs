//! Quick View shows a local step at once, waits for the cursor to rest anywhere else and ends
//! the request the cursor left (RC-110-28). The harness runs no tasks: a pause's end is the
//! message handed over here, and a request is known by its handle. The folder that is not
//! local is one inside an archive.

use std::{fs, path::Path, time::Duration};

use iced::task;
use rc_db::Db;
use rc_engine::{Command, EngineEvent, PanelId, VfsPath, settings::QUICK_VIEW_PAUSE_KEY};
use rc_vfs::BackendId;
use tempfile::TempDir;

use super::super::{QuickShown, ViewerMessage, wrap};
use super::{QuickPause, concerns, load};
use crate::app::{
    Message,
    harness::{self, Harness},
};

/// `..`, then `a.txt`, `b.txt` and `c.txt`, on the local disk.
fn files() -> TempDir {
    harness::tree(&[("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n")])
}

/// Quick View on from the left panel, which shows `conformance/` inside a 7z: `..`, `sub`,
/// `a.txt` and `b.bin`.
async fn in_an_archive() -> (TempDir, Harness) {
    let root = files();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../rc-vfs/tests/fixtures/archives/tree.7z");
    fs::copy(&fixture, root.path().join("tree.7z")).expect("copy");
    let mut harness = Harness::headless(root.path()).await;
    let inside = root.path().join("tree.7z").join("conformance");
    harness
        .app
        .engine()
        .dispatch(Command::Navigate {
            panel: None,
            path: inside.to_string_lossy().into_owned(),
        })
        .await
        .expect("into the archive");
    harness.settle().await;
    let shown = &harness.app.pane(PanelId::Left).state.path;
    assert_ne!(shown.backend, BackendId::Local, "not a local folder");
    harness.press("Ctrl+Q").await;
    (root, harness)
}

fn turn(harness: &Harness) -> u64 {
    harness.app.viewers.quick.as_ref().expect("on").turn
}

/// The entry Quick View shows or reads.
fn shown(harness: &Harness) -> Option<VfsPath> {
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    quick.path().cloned()
}

/// The entry under the left panel's cursor.
fn under_cursor(harness: &Harness) -> Option<VfsPath> {
    let pane = harness.app.pane(PanelId::Left);
    if pane.on_parent() {
        return None;
    }
    let entry = pane.entries().get(pane.cursor())?;
    Some(pane.state.path.join(&entry.name))
}

/// The handle of the request running now, when one is.
fn handle(harness: &Harness) -> Option<task::Handle> {
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    quick.running.as_ref().map(|running| running.task.clone())
}

async fn rested(harness: &mut Harness, turn: u64) {
    harness.send(wrap(ViewerMessage::QuickRested(turn))).await;
}

/// The cursor steps down and rests: its entry is asked for.
async fn step_and_rest(harness: &mut Harness) {
    harness.press("ArrowDown").await;
    let last = turn(harness);
    rested(harness, last).await;
}

#[tokio::test]
async fn a_step_on_a_local_folder_asks_at_once() {
    let root = files();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Q").await;
    harness.press("ArrowDown").await;
    assert_eq!(
        shown(&harness),
        Some(VfsPath::local(root.path().join("a.txt")))
    );
    let open = handle(&harness).expect("a.txt is opened without a pause");
    assert!(!open.is_aborted());
    // The next step takes its place and ends what a.txt still read.
    harness.press("ArrowDown").await;
    assert_eq!(
        shown(&harness),
        Some(VfsPath::local(root.path().join("b.txt")))
    );
    assert!(open.is_aborted());
    assert!(handle(&harness).is_some_and(|open| !open.is_aborted()));
}

#[tokio::test]
async fn a_step_on_a_server_asks_after_the_pause() {
    let (_root, mut harness) = in_an_archive().await;
    harness.press("ArrowDown").await;
    let entry = under_cursor(&harness).expect("an entry");
    assert_eq!(shown(&harness), None, "not before the pause");
    assert!(handle(&harness).is_none());
    let last = turn(&harness);
    rested(&mut harness, last).await;
    assert_eq!(shown(&harness), Some(entry));
    assert!(handle(&harness).is_some());
}

#[tokio::test]
async fn only_the_last_step_of_a_fast_run_starts_a_request() {
    let (_root, mut harness) = in_an_archive().await;
    // Three steps before any pause ends: `sub`, `a.txt`, `b.bin`.
    let mut turns = Vec::new();
    for _ in 0..3 {
        harness.press("ArrowDown").await;
        turns.push(turn(&harness));
        assert!(handle(&harness).is_none(), "a step asks for nothing");
    }
    let stopped_at = under_cursor(&harness).expect("the last entry");
    let last = turns.pop().expect("three steps");
    // The pauses of the steps on the way end: none of them is the last move.
    for stale in turns {
        rested(&mut harness, stale).await;
        assert!(handle(&harness).is_none(), "turn {stale} is too old");
        assert_eq!(shown(&harness), None);
    }
    rested(&mut harness, last).await;
    assert_eq!(shown(&harness), Some(stopped_at));
    let asked = handle(&harness).expect("one request for the whole run");
    // The same pause twice asks once.
    rested(&mut harness, last).await;
    assert!(!asked.is_aborted(), "not asked again");
}

#[tokio::test]
async fn the_cursor_leaving_an_unanswered_entry_ends_its_request() {
    let (_root, mut harness) = in_an_archive().await;
    step_and_rest(&mut harness).await;
    let left = shown(&harness).expect("read");
    let asked = handle(&harness).expect("a request");
    harness.press("ArrowDown").await;
    assert!(asked.is_aborted(), "nobody waits for it");
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    assert!(quick.running.is_none());
    assert!(
        matches!(quick.shown, QuickShown::Nothing),
        "its empty view went with the request"
    );
    // Back on it, it is asked for again.
    harness.press("ArrowUp").await;
    let last = turn(&harness);
    rested(&mut harness, last).await;
    assert_eq!(shown(&harness), Some(left));
    assert!(handle(&harness).is_some());
}

#[tokio::test]
async fn an_answered_entry_stays_until_the_cursor_rests_elsewhere() {
    let (_root, mut harness) = in_an_archive().await;
    step_and_rest(&mut harness).await;
    let first = shown(&harness).expect("read");
    // Its answer came.
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    let running = quick.running.as_ref().expect("a request");
    running
        .done
        .store(true, std::sync::atomic::Ordering::Relaxed);

    harness.press("ArrowDown").await;
    assert_eq!(
        shown(&harness),
        Some(first.clone()),
        "kept during the pause"
    );
    // Back before the pause ended: it is shown already, and the pause counts for nothing.
    let away = turn(&harness);
    harness.press("ArrowUp").await;
    rested(&mut harness, away).await;
    let last = turn(&harness);
    rested(&mut harness, last).await;
    assert_eq!(shown(&harness), Some(first.clone()));
    assert!(handle(&harness).is_none(), "not asked again");

    step_and_rest(&mut harness).await;
    let next = under_cursor(&harness).expect("the next entry");
    assert_ne!(next, first);
    assert_eq!(shown(&harness), Some(next));
}

#[tokio::test]
async fn with_the_pause_at_zero_a_server_step_asks_at_once() {
    let (_root, mut harness) = in_an_archive().await;
    let none = ViewerMessage::QuickPause(QuickPause(Duration::ZERO));
    harness.send(wrap(none)).await;
    harness.press("ArrowDown").await;
    let entry = under_cursor(&harness).expect("an entry");
    assert_eq!(shown(&harness), Some(entry));
    assert!(handle(&harness).is_some());
}

#[tokio::test]
async fn switching_quick_view_off_ends_its_request() {
    let root = files();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Q").await;
    harness.press("ArrowDown").await;
    let open = handle(&harness).expect("a.txt is opened");
    harness.press("Ctrl+Q").await;
    assert!(harness.app.viewers.quick.is_none());
    assert!(open.is_aborted());
    // A pause that ends after it finds no Quick View.
    harness.send(wrap(ViewerMessage::QuickRested(1))).await;
    assert!(harness.app.viewers.quick.is_none());
}

#[tokio::test]
async fn the_pause_is_the_setting_s_and_a_change_is_read() {
    let db = Db::open_in_memory().await.expect("db");
    assert_eq!(load(db.clone()).await, QuickPause::default());
    assert_eq!(QuickPause::default().0, Duration::from_millis(300));
    db.set(QUICK_VIEW_PAUSE_KEY, &0).await.expect("set");
    assert_eq!(load(db.clone()).await.0, Duration::ZERO);
    db.set(QUICK_VIEW_PAUSE_KEY, &750).await.expect("set");
    assert_eq!(load(db.clone()).await.0, Duration::from_millis(750));

    assert!(concerns(QUICK_VIEW_PAUSE_KEY));
    assert!(!concerns("viewer.text_zoom"));
    let root = files();
    let mut harness = Harness::on_db(root.path(), db).await;
    let changed = Message::Engine(EngineEvent::SettingChanged {
        key: QUICK_VIEW_PAUSE_KEY.to_owned(),
    });
    assert!(
        harness.app.viewer_intercept(&changed).is_some(),
        "read again"
    );
    let read = ViewerMessage::QuickPause(QuickPause(Duration::from_millis(750)));
    harness.send(wrap(read)).await;
    assert_eq!(
        harness.app.viewers.quick_pause.0,
        Duration::from_millis(750)
    );
}
