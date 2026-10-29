use std::time::Duration;

use crate::test_support;
use crate::{Command, DirSize, Engine, PanelId};

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
            return sizes.into_iter().collect();
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
