use super::*;
use crate::{ErrorLogQuery, LogLevel, RETAINED_TABLES};

fn line(message: &str) -> NewErrorLogEntry {
    NewErrorLogEntry {
        level: LogLevel::Warn,
        target: "rc_db::diagnostics_tests".to_owned(),
        message: message.to_owned(),
        code: None,
    }
}

#[tokio::test]
async fn store_writes_the_buffer_and_empties_it() {
    let db = Db::open_in_memory().await.expect("open");
    let mut buffer = vec![line("one"), line("two")];
    store(&db, &mut buffer).await;
    assert!(buffer.is_empty());
    let stored = db.error_log(&ErrorLogQuery::default()).await.expect("read");
    assert_eq!(stored.len(), 2);
}

#[tokio::test]
async fn a_sweep_reads_the_settings_anew() {
    let db = Db::open_in_memory().await.expect("open");
    let mut buffer: Vec<NewErrorLogEntry> = (0..300).map(|_| line("x")).collect();
    store(&db, &mut buffer).await;

    sweep_once(&db).await;
    let count = |db: Db| async move {
        db.error_log(&ErrorLogQuery {
            limit: Some(1_000),
            ..ErrorLogQuery::default()
        })
        .await
        .expect("read")
        .len()
    };
    assert_eq!(count(db.clone()).await, 300, "within the defaults");

    let table = &RETAINED_TABLES[0];
    db.set(table.rows_key, &table.floor.max_rows)
        .await
        .expect("lower");
    sweep_once(&db).await;
    assert_eq!(
        count(db.clone()).await,
        usize::try_from(table.floor.max_rows).expect("small")
    );
}

#[tokio::test]
async fn a_line_that_repeats_the_one_before_is_stored_once() {
    let db = Db::open_in_memory().await.expect("open");
    let mut recent = Recent::default();
    let mut buffer = vec![
        line("present failed"),
        line("present failed"),
        line("other"),
    ];
    take(&db, &mut buffer, &mut recent).await;
    let mut buffer = vec![
        line("other"),
        line("present failed"),
        line("present failed"),
    ];
    take(&db, &mut buffer, &mut recent).await;

    let mut stored: Vec<String> = db
        .error_log(&ErrorLogQuery::default())
        .await
        .expect("read")
        .into_iter()
        .map(|entry| entry.message)
        .collect();
    stored.sort();
    assert_eq!(stored, ["other", "present failed", "present failed"]);
}

#[tokio::test]
async fn a_flood_of_lines_is_swept_before_the_hour_is_up() {
    let db = Db::open_in_memory().await.expect("open");
    let table = &RETAINED_TABLES[0];
    db.set(table.rows_key, &table.floor.max_rows)
        .await
        .expect("lower");
    let mut recent = Recent::default();
    let mut swept = false;
    for index in 0..SWEEP_AFTER_LINES {
        let mut buffer = vec![line(&format!("line {index}"))];
        swept = take(&db, &mut buffer, &mut recent).await;
    }
    assert!(swept, "the last line reaches the threshold");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM error_log")
        .fetch_one(db.readers())
        .await
        .expect("count");
    assert_eq!(count, i64::try_from(table.floor.max_rows).expect("small"));
}

#[test]
fn without_a_runtime_nothing_is_installed() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let db = runtime
        .block_on(Db::open_in_memory())
        .expect("open in memory");
    // Outside `block_on` no runtime is current.
    assert!(!install_diagnostics(&db));
}

#[test]
fn ftp_command_traces_are_never_written() {
    assert!(carries_secrets("suppaftp", &Level::TRACE));
    assert!(carries_secrets("suppaftp::async_ftp", &Level::TRACE));
    assert!(!carries_secrets("suppaftp::async_ftp", &Level::DEBUG));
    assert!(!carries_secrets("suppaftpx", &Level::TRACE));
    assert!(!carries_secrets("rc_remote::ftp", &Level::TRACE));
}
