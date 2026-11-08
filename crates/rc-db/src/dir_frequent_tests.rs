use super::*;
use crate::{DURABLE_TABLES, retained_table};

const DAY: i64 = 24 * 60 * 60;
const START: i64 = 1_800_000_000;

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

#[tokio::test]
async fn frequent_folders_count_visits() {
    let db = db().await;
    for _ in 0..3 {
        db.record_frequent("a", START).await.expect("a");
    }
    db.record_frequent("b", START).await.expect("b");
    let dirs = db.frequent_dirs(10, START).await.expect("read");
    assert_eq!(
        dirs,
        vec![
            FrequentDir {
                location: "a".to_owned(),
                visits: 3
            },
            FrequentDir {
                location: "b".to_owned(),
                visits: 1
            },
        ]
    );
    assert_eq!(db.frequent_dirs(1, START).await.expect("one").len(), 1);
    assert!(db.frequent_dirs(0, START).await.expect("none").is_empty());
}

#[tokio::test]
async fn counts_halve_after_thirty_days() {
    let db = db().await;
    for _ in 0..8 {
        db.record_frequent("a", START).await.expect("visit");
    }
    let later = START + 30 * DAY;
    assert_eq!(db.frequent_dirs(5, later).await.expect("read")[0].visits, 4);
    assert_eq!(
        db.frequent_dirs(5, START + 61 * DAY).await.expect("read")[0].visits,
        2
    );
    // A visit after the halving counts on from the halved number.
    db.record_frequent("a", later).await.expect("again");
    assert_eq!(db.frequent_dirs(5, later).await.expect("read")[0].visits, 5);
    // Faded to nothing: left out.
    assert!(
        db.frequent_dirs(5, later + 400 * DAY)
            .await
            .expect("read")
            .is_empty()
    );
}

#[test]
fn decay_halves_once_per_whole_period() {
    assert_eq!(decayed(8, START, START + 29 * DAY), (8, START));
    assert_eq!(decayed(8, START, START + 30 * DAY), (4, START + 30 * DAY));
    assert_eq!(decayed(8, START, START + 95 * DAY), (1, START + 90 * DAY));
    assert_eq!(decayed(8, START, START - DAY), (8, START));
    assert_eq!(decayed(i64::MAX, 0, i64::MAX).0, 0);
}

#[tokio::test]
async fn an_overlong_location_is_not_counted() {
    let db = db().await;
    db.record_frequent(&"x".repeat(9000), START)
        .await
        .expect("skipped");
    assert!(db.frequent_dirs(5, START).await.expect("read").is_empty());
}

#[test]
fn the_table_is_retained() {
    assert!(retained_table("dir_frequent").is_ok());
    assert!(
        !DURABLE_TABLES
            .iter()
            .any(|(name, _)| *name == "dir_frequent")
    );
}
