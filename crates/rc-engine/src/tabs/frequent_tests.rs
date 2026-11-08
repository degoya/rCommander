use std::time::Duration;

use super::*;
use crate::test_support;
use crate::{Command, PanelRequest};

async fn go(engine: &Engine, path: &str) {
    engine
        .dispatch(Command::Navigate {
            panel: None,
            path: path.into(),
        })
        .await
        .expect("navigates");
}

/// The most visited folders, read from the database, once `ready` holds for them.
async fn known(engine: &Engine, ready: impl Fn(&[FrequentFolder]) -> bool) -> Vec<FrequentFolder> {
    for _ in 0..500 {
        engine.read_frequent().await;
        let frequent = engine.frequent();
        if ready(&frequent) {
            return frequent;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the visits were not counted");
}

#[tokio::test]
async fn frequent_folders_count_visits() {
    let (root, engine) = test_support::engine_with_db(&[("a/", ""), ("b/", "")]).await;
    for _ in 0..3 {
        go(&engine, "a").await;
        go(&engine, "..").await;
    }
    go(&engine, "b").await;
    let a = root.path().join("a");
    let frequent = known(&engine, |frequent| {
        let visits: Vec<i64> = frequent.iter().map(|folder| folder.visits).collect();
        visits == [3, 3, 1]
            && frequent
                .iter()
                .any(|folder| folder.path.path == a && folder.visits == 3)
    })
    .await;
    // The root was shown three times, `a` three times, `b` once.
    assert_eq!(frequent.last().map(|folder| folder.visits), Some(1));
    assert!(
        frequent
            .iter()
            .any(|folder| folder.path.path == root.path().join("a") && folder.visits == 3)
    );
    let listed = engine
        .dispatch(Command::HistoryList(PanelRequest { panel: None }))
        .await
        .expect("history");
    let Outcome::History {
        frequent: in_list, ..
    } = listed
    else {
        panic!("a history");
    };
    assert_eq!(in_list, frequent);
    let at = frequent
        .iter()
        .position(|folder| folder.path.path == root.path().join("a"))
        .expect("a");
    engine
        .dispatch(Command::HistoryOpenFrequent {
            panel: None,
            index: at,
        })
        .await
        .expect("opens");
    assert_eq!(engine.panel_path(PanelId::Left).path, root.path().join("a"));
    let error = engine
        .dispatch(Command::HistoryOpenFrequent {
            panel: None,
            index: 99,
        })
        .await
        .expect_err("out of range");
    assert_eq!(error.code(), "history_out_of_range");
}

#[tokio::test]
async fn zero_shown_records_nothing() {
    let (_root, engine) = test_support::engine_with_db(&[("a/", "")]).await;
    engine
        .dispatch(Command::SetSetting {
            key: FREQUENT_SHOWN_KEY.into(),
            value: serde_json::json!(0),
        })
        .await
        .expect("stored");
    go(&engine, "a").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(engine.frequent().is_empty());
    let db = engine.db().expect("db");
    assert!(
        db.frequent_dirs(10, now()).await.expect("read").is_empty(),
        "nothing was counted"
    );
}
