use crate::{Db, RETAINED_TABLES, RetentionLimits};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

const SWAP: &[(&str, &str)] = &[("a", ".rc-rename-7-1"), ("b", "a"), (".rc-rename-7-1", "b")];

#[tokio::test]
async fn a_run_is_logged_in_order_and_not_done() {
    let db = db().await;
    assert_eq!(db.latest_rename_run().await.expect("read"), None);
    db.log_rename_run("7", "{\"dir\":1}", SWAP)
        .await
        .expect("log");
    let run = db.latest_rename_run().await.expect("read").expect("a run");
    assert_eq!((run.run.as_str(), run.dir.as_str()), ("7", "{\"dir\":1}"));
    assert_eq!(run.total, 3);
    assert!(run.is_complete());
    assert_eq!(run.marked(), 0);
    let pairs: Vec<(&str, &str)> = run
        .steps
        .iter()
        .map(|step| (step.from.as_str(), step.to.as_str()))
        .collect();
    assert_eq!(pairs, SWAP);
}

#[tokio::test]
async fn marks_are_a_prefix_and_can_be_set_back() {
    let db = db().await;
    db.log_rename_run("7", "d", SWAP).await.expect("log");
    db.mark_rename_step("7", 0, true).await.expect("mark");
    db.mark_rename_step("7", 1, true).await.expect("mark");
    let run = db.latest_rename_run().await.expect("read").expect("run");
    assert_eq!(run.marked(), 2);
    db.mark_rename_step("7", 1, false).await.expect("unmark");
    assert_eq!(
        db.latest_rename_run()
            .await
            .expect("read")
            .expect("run")
            .marked(),
        1
    );
    db.set_rename_marks("7", 3).await.expect("set");
    assert_eq!(
        db.latest_rename_run()
            .await
            .expect("read")
            .expect("run")
            .marked(),
        3
    );
    db.set_rename_marks("7", 0).await.expect("set");
    assert_eq!(
        db.latest_rename_run()
            .await
            .expect("read")
            .expect("run")
            .marked(),
        0
    );
}

#[tokio::test]
async fn the_latest_run_wins_and_a_tag_is_logged_once() {
    let db = db().await;
    db.log_rename_run("1", "d", SWAP).await.expect("log");
    db.log_rename_run("2", "e", &[("x", "y")])
        .await
        .expect("log");
    let run = db.latest_rename_run().await.expect("read").expect("run");
    assert_eq!((run.run.as_str(), run.total), ("2", 1));
    assert!(db.log_rename_run("2", "e", &[("x", "y")]).await.is_err());
    // Nothing of the refused second write is left.
    assert_eq!(
        db.latest_rename_run()
            .await
            .expect("read")
            .expect("run")
            .steps
            .len(),
        1
    );
    db.log_rename_run("3", "f", &[]).await.expect("empty");
    assert_eq!(
        db.latest_rename_run()
            .await
            .expect("read")
            .expect("run")
            .run,
        "2",
        "a run without steps is not logged"
    );
}

#[tokio::test]
async fn a_run_the_sweep_cut_is_incomplete() {
    let db = db().await;
    db.log_rename_run("7", "d", SWAP).await.expect("log");
    {
        // What the sweep does to the oldest rows over the limit.
        let mut writer = db.writer().await;
        sqlx::query(
            "DELETE FROM rename_log WHERE id IN (SELECT id FROM rename_log ORDER BY id LIMIT 1)",
        )
        .execute(&mut *writer)
        .await
        .expect("cut");
    }
    let run = db.latest_rename_run().await.expect("read").expect("run");
    assert_eq!((run.total, run.steps.len()), (3, 2));
    assert!(!run.is_complete());
}

#[test]
fn the_log_is_retained_with_the_undo_bounds() {
    let table = RETAINED_TABLES
        .iter()
        .find(|table| table.table == "rename_log")
        .expect("rename_log is retained");
    assert_eq!(table.time_column, "recorded_at");
    assert_eq!(
        table.defaults,
        RetentionLimits {
            max_rows: 100_000,
            max_age_days: 30
        }
    );
}
