use super::*;
use crate::{
    DB_FILE,
    error_log::{LogLevel, NewErrorLogEntry},
};

/// `count` error lines of a few hundred bytes each, oldest first.
fn lines(count: usize) -> Vec<NewErrorLogEntry> {
    (0..count)
        .map(|index| NewErrorLogEntry {
            level: LogLevel::Error,
            target: "rc_db::prune_tests".to_owned(),
            message: format!("line {index:05}: {}", "x".repeat(300)),
            code: Some("io".to_owned()),
        })
        .collect()
}

async fn count(db: &Db) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM error_log")
        .fetch_one(db.readers())
        .await
        .expect("count")
}

async fn page_count(db: &Db) -> i64 {
    let mut writer = db.writer().await;
    sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&mut *writer)
        .await
        .expect("page_count")
}

#[tokio::test]
async fn twenty_thousand_lines_shrink_to_the_newest_max_rows() {
    let db = Db::open_in_memory().await.expect("open");
    db.append_error_log(&lines(20_000)).await.expect("append");
    assert_eq!(count(&db).await, 20_000);

    let settings = RetentionSettings::defaults();
    let max_rows = settings.limits("error_log").expect("retained").max_rows;
    let report = db.prune(&settings).await.expect("prune");

    assert_eq!(report.deleted, 20_000 - max_rows);
    assert_eq!(count(&db).await, i64::try_from(max_rows).expect("small"));
    let (oldest, newest): (String, String) =
        sqlx::query_as("SELECT MIN(message), MAX(message) FROM error_log")
            .fetch_one(db.readers())
            .await
            .expect("range");
    assert!(oldest.starts_with("line 15000:"), "{oldest}");
    assert!(newest.starts_with("line 19999:"), "{newest}");

    let again = db.prune(&settings).await.expect("prune again");
    assert_eq!(again, PruneReport::default(), "nothing left to do");
}

#[tokio::test]
async fn lines_older_than_the_age_limit_go_whatever_the_count() {
    let db = Db::open_in_memory().await.expect("open");
    {
        let mut writer = db.writer().await;
        for days in [40, 31, 29, 1] {
            sqlx::query(
                "INSERT INTO error_log (recorded_at, level, target, message) \
                 VALUES (strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?), 'warn', 't', ?)",
            )
            .bind(format!("-{days} days"))
            .bind(format!("{days} days old"))
            .execute(&mut *writer)
            .await
            .expect("insert");
        }
    }
    let report = db
        .prune(&RetentionSettings::defaults())
        .await
        .expect("prune");
    assert_eq!(report.deleted, 2);
    let kept: Vec<String> = sqlx::query_scalar("SELECT message FROM error_log ORDER BY id")
        .fetch_all(db.readers())
        .await
        .expect("kept");
    assert_eq!(kept, ["29 days old", "1 days old"]);
}

#[tokio::test]
async fn more_aged_rows_than_one_batch_all_go() {
    let db = Db::open_in_memory().await.expect("open");
    let rows = PRUNE_BATCH * 2 + 7;
    {
        let mut writer = db.writer().await;
        sqlx::query(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?) \
             INSERT INTO error_log (recorded_at, level, target, message) \
             SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-90 days'), 'warn', 't', 'old' FROM n",
        )
        .bind(i64::try_from(rows).expect("small"))
        .execute(&mut *writer)
        .await
        .expect("insert");
    }
    let report = db
        .prune(&RetentionSettings::defaults())
        .await
        .expect("prune");
    assert_eq!(report.deleted, rows);
    assert_eq!(count(&db).await, 0);
}

#[tokio::test]
async fn lowered_limits_apply_on_the_next_prune() {
    let db = Db::open_in_memory().await.expect("open");
    db.append_error_log(&lines(500)).await.expect("append");
    let table = crate::retained_table("error_log").expect("retained");
    let lowered = table
        .check(RetentionLimits {
            max_rows: 100,
            max_age_days: 30,
        })
        .expect("in range");
    let settings = RetentionSettings::defaults().with(lowered);
    db.prune(&settings).await.expect("prune");
    assert_eq!(count(&db).await, 100);
}

/// The file gives the space back: `auto_vacuum = INCREMENTAL` and `incremental_vacuum` after a
/// prune that freed many pages.
#[tokio::test]
async fn a_large_prune_makes_the_file_smaller() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(DB_FILE);
    let db = Db::open(&path).await.expect("open");
    db.append_error_log(&lines(20_000)).await.expect("append");
    let before = page_count(&db).await;

    let report = db
        .prune(&RetentionSettings::defaults())
        .await
        .expect("prune");

    let after = page_count(&db).await;
    assert!(report.freed_pages > 0, "{report:?}");
    assert!(
        after < before,
        "page_count {before} -> {after} after deleting {} rows",
        report.deleted
    );
    let free: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(db.readers())
        .await
        .expect("freelist");
    assert!(free < VACUUM_FREE_PAGES, "{free} pages left free");
    db.close().await;
}

/// Free pages a sweep cut short left behind go back at the next one, though it deletes nothing,
/// and more than one chunk of the vacuum's comes back.
#[tokio::test]
async fn free_pages_left_behind_go_back_in_chunks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join(DB_FILE)).await.expect("open");
    db.append_error_log(&lines(40_000)).await.expect("append");
    {
        let mut writer = db.writer().await;
        sqlx::query("DELETE FROM error_log")
            .execute(&mut *writer)
            .await
            .expect("delete");
    }

    let report = db
        .prune(&RetentionSettings::defaults())
        .await
        .expect("prune");

    assert_eq!(report.deleted, 0);
    assert!(report.freed_pages > 2_048, "{report:?}");
    let free: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(db.readers())
        .await
        .expect("freelist");
    assert!(free < VACUUM_FREE_PAGES, "{free} pages left free");
    db.close().await;
}

/// A batch must not read the whole table: on a flooded `error_log` (41 million rows, 12 GB) a
/// `SCAN` per batch made the sweep run for hours and hold the writer at every start.
#[tokio::test]
async fn no_delete_of_a_batch_scans_the_whole_table() {
    let db = Db::open_in_memory().await.expect("open");
    for table in crate::RETAINED_TABLES {
        for sql in [aged_sql(table), over_sql(table)] {
            let plan: Vec<(i64, i64, i64, String)> =
                sqlx::query_as(AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
                    .bind("-1 days")
                    .bind(1_i64)
                    .fetch_all(db.readers())
                    .await
                    .expect("plan");
            let scan = format!("SCAN {}", table.table);
            assert!(
                plan.iter().all(|(_, _, _, detail)| detail != &scan),
                "{sql}: {plan:?}"
            );
        }
    }
}
