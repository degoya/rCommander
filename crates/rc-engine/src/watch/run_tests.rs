//! The watcher on a real temporary folder: a new file shows, a burst is a few readings, a lost
//! notice is one, a size change only with its switch, a deleted folder is left, and nothing is
//! read in the background until the return.

use std::{
    fs,
    io::Write as _,
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use serde_json::Value;
use tempfile::TempDir;

use crate::{
    Command, CursorTarget, Engine, PanelId, VfsPath,
    settings::{WATCH_BACKGROUND_KEY, WATCH_CHANGES_KEY, WATCH_KEY},
    test_support::{self, run_panel},
    watch::{
        Signal,
        native::{Change, watch_key},
    },
};

const LEFT: PanelId = PanelId::Left;

async fn set(engine: &Engine, key: &str, on: bool) {
    engine
        .dispatch(Command::SetSetting {
            key: key.into(),
            value: Value::Bool(on),
        })
        .await
        .expect("stored");
}

/// Waits until `check` holds, at most `limit`; how long it took.
async fn until(what: &str, limit: Duration, check: impl Fn() -> bool) -> Duration {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < limit, "not within {limit:?}: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    started.elapsed()
}

fn lists(engine: &Engine, panel: PanelId, name: &str) -> bool {
    engine.panel(panel).index_of(name).is_some()
}

/// `engine` with `panels.watch` on and its watcher running, once the left panel is watched.
async fn watch(engine: &Engine) {
    set(engine, WATCH_KEY, true).await;
    engine.start_watching();
    until("the left panel is watched", Duration::from_secs(5), || {
        engine.watch_quiet(LEFT)
    })
    .await;
}

/// A folder of `entries`, both panels on it, watched.
async fn watching(entries: &[(&str, &str)]) -> (TempDir, Engine) {
    let (root, engine) = test_support::engine_with_db(entries).await;
    watch(&engine).await;
    (root, engine)
}

fn append(path: &Path) {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open");
    file.write_all(b"more").expect("write");
}

#[tokio::test]
async fn a_new_file_shows_at_once_and_cursor_and_selection_stay() {
    let (root, engine) = watching(&[("a.txt", "a"), ("b.txt", "b")]).await;
    run_panel(
        &engine,
        Command::SetCursor {
            panel: Some(LEFT),
            target: CursorTarget::Name("b.txt".into()),
        },
    )
    .await;
    run_panel(
        &engine,
        Command::ToggleSelection {
            panel: Some(LEFT),
            names: vec!["a.txt".into()],
        },
    )
    .await;
    fs::write(root.path().join("c.txt"), "c").expect("write");
    let took = until("c.txt is listed", Duration::from_secs(2), || {
        lists(&engine, LEFT, "c.txt")
    })
    .await;
    eprintln!("RC-110-02: a new file was listed after {took:?}");
    let left = engine.panel(LEFT);
    assert_eq!(
        left.cursor_entry().map(|entry| entry.name.as_str()),
        Some("b.txt")
    );
    assert!(left.selection.contains("a.txt"));
    // The other panel shows the same folder: one watch serves both.
    until(
        "the right panel lists c.txt",
        Duration::from_secs(2),
        || lists(&engine, PanelId::Right, "c.txt"),
    )
    .await;
}

#[tokio::test]
async fn ten_thousand_files_in_one_burst_are_a_few_readings_then_none() {
    let (root, engine) = watching(&[]).await;
    let before = engine.watch_reads(LEFT);
    for index in 0..10_000 {
        fs::write(root.path().join(format!("f{index:05}")), b"").expect("write");
    }
    until("every file is listed", Duration::from_secs(30), || {
        lists(&engine, LEFT, "f09999") && engine.panel(LEFT).entries.len() >= 10_000
    })
    .await;
    // What was noticed during the last reading is read once more, then all is quiet.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let settled = engine.watch_reads(LEFT);
    eprintln!(
        "RC-110-02: 10 000 new files took {} readings",
        settled - before
    );
    assert!(settled - before <= 8, "{} readings", settled - before);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(engine.watch_reads(LEFT), settled, "no change, no reading");
}

#[tokio::test]
async fn a_full_channel_or_a_lost_notice_is_one_full_reading() {
    let (_root, engine) = watching(&[("a.txt", "a")]).await;
    let before = engine.watch_reads(LEFT);
    engine.inner.watch.overflow.store(true, Ordering::Release);
    for _ in 0..3 {
        assert!(engine.inner.watch.send(Signal::Rescan));
    }
    until("one reading", Duration::from_secs(2), || {
        engine.watch_reads(LEFT) > before
    })
    .await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(engine.watch_reads(LEFT), before + 1);
}

#[tokio::test]
async fn a_size_change_is_read_only_with_watch_changes() {
    let (root, engine) = watching(&[("a.txt", "a")]).await;
    let before = engine.watch_reads(LEFT);
    append(&root.path().join("a.txt"));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    // On macOS the write arrives as a creation too (FSEvents keeps a path's flags together);
    // a.txt is listed, so it is no new entry either.
    assert_eq!(
        engine.watch_reads(LEFT),
        before,
        "without panels.watch_changes"
    );
    // The watcher saw it, so coming back to the window reads the panel (RC-100-69).
    assert!(!engine.watch_quiet(LEFT));
    set(&engine, WATCH_CHANGES_KEY, true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = engine.watch_reads(LEFT);
    append(&root.path().join("a.txt"));
    until("one reading", Duration::from_secs(2), || {
        engine.watch_reads(LEFT) > before
    })
    .await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(engine.watch_reads(LEFT), before + 1);
}

/// A creation notice for `name` in `root`, as FSEvents sends one for a write to a recent file.
fn created(root: &Path, name: &str) -> Signal {
    Signal::Changed {
        paths: vec![watch_key(root).join(name)],
        change: Change::Created,
    }
}

#[tokio::test]
async fn a_creation_notice_for_a_listed_name_is_a_change_of_content() {
    let (root, engine) = watching(&[("a.txt", "a")]).await;
    let before = engine.watch_reads(LEFT);
    assert!(engine.inner.watch.send(created(root.path(), "a.txt")));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(engine.watch_reads(LEFT), before, "a.txt is listed");
    assert!(!engine.watch_quiet(LEFT));
    assert!(engine.inner.watch.send(created(root.path(), "b.txt")));
    until("one reading", Duration::from_secs(2), || {
        engine.watch_reads(LEFT) > before
    })
    .await;
    let before = engine.watch_reads(LEFT);
    set(&engine, WATCH_CHANGES_KEY, true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(engine.inner.watch.send(created(root.path(), "a.txt")));
    until(
        "a reading with panels.watch_changes",
        Duration::from_secs(2),
        || engine.watch_reads(LEFT) > before,
    )
    .await;
}

#[tokio::test]
async fn a_deleted_watched_folder_leaves_for_the_nearest_parent() {
    let (root, engine) = test_support::engine_with_db(&[("outer/inner/x.txt", "x")]).await;
    let inner = root.path().join("outer").join("inner");
    run_panel(
        &engine,
        Command::Navigate {
            panel: Some(LEFT),
            path: inner.to_string_lossy().into_owned(),
        },
    )
    .await;
    watch(&engine).await;
    fs::remove_dir_all(root.path().join("outer")).expect("remove");
    let home = VfsPath::local(root.path());
    until("the panel is on the root", Duration::from_secs(3), || {
        engine.panel_path(LEFT) == home
    })
    .await;
}

#[tokio::test]
async fn in_the_background_nothing_is_read_until_the_return() {
    let (root, engine) = watching(&[("a.txt", "a")]).await;
    set(&engine, WATCH_BACKGROUND_KEY, false).await;
    engine.set_foreground(false);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = engine.watch_reads(LEFT);
    fs::write(root.path().join("c.txt"), "c").expect("write");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(engine.watch_reads(LEFT), before);
    assert!(!lists(&engine, LEFT, "c.txt"));
    engine.set_foreground(true);
    until("c.txt is listed", Duration::from_secs(2), || {
        lists(&engine, LEFT, "c.txt")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(engine.watch_reads(LEFT), before + 1, "read once");
}

#[tokio::test]
async fn the_switch_and_the_command_turn_the_watcher_off() {
    let (root, engine) = watching(&[("a.txt", "a")]).await;
    engine
        .dispatch(Command::ToggleWatch {})
        .await
        .expect("switched");
    until(
        "the panel is no longer watched",
        Duration::from_secs(2),
        || !engine.watch_quiet(LEFT),
    )
    .await;
    let before = engine.watch_reads(LEFT);
    fs::write(root.path().join("c.txt"), "c").expect("write");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(engine.watch_reads(LEFT), before);
}
