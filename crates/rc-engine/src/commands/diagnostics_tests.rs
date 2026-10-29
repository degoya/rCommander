use crate::test_support;
use crate::test_support::engine_with_db;
use crate::{Command, EngineError, LogLevel, Outcome, RetentionLimits};

#[tokio::test]
async fn without_a_database_the_commands_say_so() {
    let (_root, engine) = test_support::engine(&[]).await;
    let error = engine
        .dispatch(Command::GetRetention)
        .await
        .expect_err("no database");
    assert_eq!(error.code(), "database_unavailable");
}

#[tokio::test]
async fn an_empty_error_log_reads_and_clears() {
    let (_root, engine) = engine_with_db(&[]).await;
    let read = engine
        .dispatch(Command::ErrorLog(crate::ErrorLogRequest {
            level: Some(LogLevel::Error),
            contains: None,
            limit: None,
        }))
        .await
        .expect("read");
    assert_eq!(read, Outcome::ErrorLog { entries: vec![] });
    let cleared = engine
        .dispatch(Command::ClearErrorLog)
        .await
        .expect("clear");
    assert_eq!(cleared, Outcome::ErrorLogCleared { removed: 0 });
}

#[tokio::test]
async fn set_retention_changes_one_limit_and_keeps_the_other() {
    let (_root, engine) = engine_with_db(&[]).await;
    let changed = engine
        .dispatch(Command::SetRetention(crate::RetentionRequest {
            table: "error_log".into(),
            max_rows: Some(1_000),
            max_age_days: None,
        }))
        .await
        .expect("set");
    let Outcome::Retention { tables } = changed else {
        panic!("retention, got {changed:?}");
    };
    assert_eq!(
        tables[0].limits,
        RetentionLimits {
            max_rows: 1_000,
            max_age_days: 30
        }
    );
    let read = engine.dispatch(Command::GetRetention).await.expect("read");
    assert_eq!(read, Outcome::Retention { tables });
}

#[tokio::test]
async fn set_retention_refuses_unlimited_and_unknown_tables() {
    let (_root, engine) = engine_with_db(&[]).await;
    let error = engine
        .dispatch(Command::SetRetention(crate::RetentionRequest {
            table: "error_log".into(),
            max_rows: Some(u64::MAX),
            max_age_days: None,
        }))
        .await
        .expect_err("over the ceiling");
    assert!(
        matches!(error, EngineError::RetentionOutOfRange { max: 50_000, .. }),
        "{error:?}"
    );
    assert_eq!(error.code(), "retention_out_of_range");

    let error = engine
        .dispatch(Command::SetRetention(crate::RetentionRequest {
            table: "settings".into(),
            max_rows: Some(1_000),
            max_age_days: None,
        }))
        .await
        .expect_err("durable table");
    assert_eq!(error.code(), "retention_unknown_table");
}
