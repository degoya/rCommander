use crate::{Db, RETAINED_TABLES, RetentionLimits};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

#[tokio::test]
async fn a_job_is_kept_marked_and_forgotten() {
    let db = db().await;
    assert!(db.transfers().await.expect("read").is_empty());
    let first = db.save_transfer("{\"op\":\"copy\"}").await.expect("save");
    let second = db.save_transfer("{\"op\":\"move\"}").await.expect("save");
    assert!(second > first);
    db.mark_transfer(first, "{\"size\":5}").await.expect("mark");
    let kept = db.transfers().await.expect("read");
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].id, first);
    assert_eq!(kept[0].spec, "{\"op\":\"copy\"}");
    assert_eq!(kept[0].file.as_deref(), Some("{\"size\":5}"));
    assert_eq!(kept[1].file, None, "not marked yet");
    assert!(kept[0].recorded_at.ends_with('Z'));
    db.forget_transfer(first).await.expect("forget");
    let kept = db.transfers().await.expect("read");
    assert_eq!(kept.iter().map(|row| row.id).collect::<Vec<_>>(), [second]);
    // A row that is gone is neither marked nor forgotten twice by mistake.
    db.mark_transfer(first, "{}")
        .await
        .expect("mark a gone row");
    db.forget_transfer(first).await.expect("forget a gone row");
    assert_eq!(db.transfers().await.expect("read").len(), 1);
}

#[tokio::test]
async fn the_migration_refuses_what_is_not_json() {
    let db = db().await;
    assert!(db.save_transfer("not json").await.is_err());
    let id = db.save_transfer("{}").await.expect("save");
    assert!(db.mark_transfer(id, "{broken").await.is_err());
    assert_eq!(db.transfers().await.expect("read")[0].file, None);
}

#[test]
fn the_jobs_are_retained() {
    let table = RETAINED_TABLES
        .iter()
        .find(|table| table.table == "transfer_jobs")
        .expect("transfer_jobs is retained");
    assert_eq!(table.time_column, "recorded_at");
    assert_eq!(
        table.defaults,
        RetentionLimits {
            max_rows: 200,
            max_age_days: 30
        }
    );
}
