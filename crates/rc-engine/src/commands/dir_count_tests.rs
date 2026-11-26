use std::time::{Duration, Instant};

use tokio::sync::broadcast::error::TryRecvError;

use super::SIZES_EVERY;
use crate::test_support;
use crate::{Command, DirSize, Engine, EngineEvent, PanelChange, PanelId};

/// `root/` with `big/` (100 bytes), `small/` (1 byte) and a file.
async fn engine() -> (tempfile::TempDir, Engine) {
    let big = "x".repeat(100);
    test_support::engine(&[("big/f", big.as_str()), ("small/f", "x"), ("z.txt", "z")]).await
}

/// The panel's counted sizes once nothing is counting any more.
async fn settled(engine: &Engine) -> Vec<(String, DirSize)> {
    for _ in 0..200 {
        let sizes = engine.panel(PanelId::Left).dir_sizes;
        if !sizes.values().any(|size| *size == DirSize::Counting) {
            return sizes
                .iter()
                .map(|(name, size)| (name.clone(), *size))
                .collect();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the count did not end");
}

#[tokio::test]
async fn space_selects_and_counts_the_folder() {
    let (_root, engine) = engine().await;
    engine
        .dispatch(Command::SelectAndSize { panel: None })
        .await
        .expect("space");
    let panel = engine.panel(PanelId::Left);
    assert!(panel.selection.contains("big"));
    assert_eq!(panel.cursor, 1, "the cursor moves on as with Insert");
    assert_eq!(
        settled(&engine).await,
        vec![("big".to_owned(), DirSize::Bytes(100))]
    );
}

/// `selection.space_moves_down` off: Space flips the selection and the cursor stays.
#[tokio::test]
async fn space_keeps_the_cursor_when_the_setting_is_off() {
    let (_root, engine) = engine().await;
    let db = rc_db::Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db));
    engine
        .dispatch(Command::SetSetting {
            key: crate::settings::SPACE_MOVES_DOWN_KEY.into(),
            value: serde_json::json!(false),
        })
        .await
        .expect("set");
    engine
        .dispatch(Command::SelectAndSize { panel: None })
        .await
        .expect("space");
    let panel = engine.panel(PanelId::Left);
    assert!(panel.selection.contains("big"));
    assert_eq!(panel.cursor, 0, "the cursor stays on the entry");
    settled(&engine).await;
}

#[tokio::test]
async fn every_folder_is_counted_and_a_new_listing_forgets() {
    let (_root, engine) = engine().await;
    engine
        .dispatch(Command::CountDirContent { panel: None })
        .await
        .expect("count");
    assert_eq!(
        settled(&engine).await,
        vec![
            ("big".to_owned(), DirSize::Bytes(100)),
            ("small".to_owned(), DirSize::Bytes(1)),
        ]
    );
    engine
        .dispatch(Command::Refresh { panel: None })
        .await
        .expect("refresh");
    assert!(engine.panel(PanelId::Left).dir_sizes.is_empty());
}

#[tokio::test]
async fn cancel_drops_what_is_still_counting() {
    let (_root, engine) = engine().await;
    engine
        .dispatch(Command::CountDirContent { panel: None })
        .await
        .expect("count");
    engine
        .dispatch(Command::CancelDirSizes { panel: None })
        .await
        .expect("cancel");
    let sizes = engine.panel(PanelId::Left).dir_sizes;
    assert!(!sizes.values().any(|size| *size == DirSize::Counting));
}

/// RC-110-29: the counted sizes are shared, not copied, with every copy of the state, and a
/// cursor step leaves the map the same one - the window tells by the pointer that nothing was
/// counted since.
#[tokio::test]
async fn copies_of_the_state_share_the_counted_sizes() {
    let (_root, engine) = engine().await;
    engine
        .dispatch(Command::CountDirContent { panel: None })
        .await
        .expect("count");
    settled(&engine).await;
    let before = engine.panel(PanelId::Left);
    engine
        .dispatch(Command::MoveCursor {
            panel: None,
            step: crate::CursorStep::Down,
        })
        .await
        .expect("cursor");
    let after = engine.panel(PanelId::Left);
    assert_ne!(before.cursor, after.cursor);
    assert!(std::sync::Arc::ptr_eq(&before.dir_sizes, &after.dir_sizes));
    // A copy handed out keeps its sizes when the engine counts on.
    engine
        .dispatch(Command::Refresh { panel: None })
        .await
        .expect("refresh");
    assert_eq!(before.dir_sizes.len(), 2);
    assert!(engine.panel(PanelId::Left).dir_sizes.is_empty());
}

/// RC-110-63: Alt+Shift+Enter over 10 000 folders shows their sizes in batches - the event that
/// marks them counting and at most one per [`SIZES_EVERY`] after it, so a window that reads every
/// event never falls behind - and the sizes add up to what the folders hold.
#[tokio::test]
async fn ten_thousand_folders_are_shown_in_batches() {
    const FOLDERS: usize = 10_000;
    let root = tempfile::tempdir().expect("temp dir");
    let mut expected = 0_u64;
    for index in 0..FOLDERS {
        let folder = root.path().join(format!("d{index:05}"));
        std::fs::create_dir(&folder).expect("mkdir");
        let bytes = index % 7;
        std::fs::write(folder.join("f"), "x".repeat(bytes)).expect("write");
        expected += bytes as u64;
    }
    let engine = test_support::headless(root.path()).await;
    let mut events = engine.subscribe();
    let started = Instant::now();
    engine
        .dispatch(Command::CountDirContent { panel: None })
        .await
        .expect("count");
    let mut shown = 0_usize;
    let mut drain = |events: &mut tokio::sync::broadcast::Receiver<EngineEvent>| loop {
        match events.try_recv() {
            Ok(EngineEvent::Panel {
                panel: PanelId::Left,
                change: PanelChange::Listing,
            }) => shown += 1,
            Ok(_) => {}
            Err(TryRecvError::Empty) => break,
            Err(error) => panic!("the window would have fallen behind: {error}"),
        }
    };
    let sizes = loop {
        drain(&mut events);
        let sizes = engine.panel(PanelId::Left).dir_sizes;
        if sizes.len() == FOLDERS && !sizes.values().any(|size| *size == DirSize::Counting) {
            break sizes;
        }
        assert!(
            started.elapsed() < Duration::from_secs(240),
            "the count did not end"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let elapsed = started.elapsed();
    // The last batch's event follows its sizes.
    tokio::time::sleep(Duration::from_millis(50)).await;
    drain(&mut events);
    let batches = elapsed.as_millis() / SIZES_EVERY.as_millis();
    let most = usize::try_from(batches).expect("a few batches") + 2;
    assert!(
        (2..=most).contains(&shown),
        "{shown} events in {elapsed:?}, at most {most}"
    );
    let total: u64 = sizes
        .values()
        .map(|size| match size {
            DirSize::Bytes(bytes) => *bytes,
            DirSize::Counting => 0,
        })
        .sum();
    assert_eq!(total, expected);
}
