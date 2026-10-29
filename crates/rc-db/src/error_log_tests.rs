use super::*;

fn line(level: LogLevel, target: &str, message: &str, code: Option<&str>) -> NewErrorLogEntry {
    NewErrorLogEntry {
        level,
        target: target.to_owned(),
        message: message.to_owned(),
        code: code.map(str::to_owned),
    }
}

async fn seeded() -> Db {
    let db = Db::open_in_memory().await.expect("open");
    db.append_error_log(&[
        line(LogLevel::Warn, "rc_mcp::http", "a refused origin", None),
        line(
            LogLevel::Error,
            "rc_vfs::local",
            "Permission denied: C:\\x",
            Some("permission_denied"),
        ),
        line(LogLevel::Error, "rc_engine", "listing failed", Some("io")),
    ])
    .await
    .expect("append");
    db
}

#[tokio::test]
async fn lines_read_back_newest_first() {
    let db = seeded().await;
    let entries = db.error_log(&ErrorLogQuery::default()).await.expect("read");
    let messages: Vec<&str> = entries.iter().map(|entry| entry.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "listing failed",
            "Permission denied: C:\\x",
            "a refused origin"
        ]
    );
    assert_eq!(entries[0].code.as_deref(), Some("io"));
    assert_eq!(entries[2].level, LogLevel::Warn);
    assert!(
        entries[0].recorded_at.ends_with('Z'),
        "{}",
        entries[0].recorded_at
    );
}

#[tokio::test]
async fn a_read_filters_by_level_text_and_limit() {
    let db = seeded().await;
    let errors = db
        .error_log(&ErrorLogQuery {
            level: Some(LogLevel::Error),
            ..ErrorLogQuery::default()
        })
        .await
        .expect("read");
    assert_eq!(errors.len(), 2);

    let by_text = db
        .error_log(&ErrorLogQuery {
            contains: Some("PERMISSION".to_owned()),
            ..ErrorLogQuery::default()
        })
        .await
        .expect("read");
    assert_eq!(by_text.len(), 1);
    assert_eq!(by_text[0].target, "rc_vfs::local");

    let by_target = db
        .error_log(&ErrorLogQuery {
            contains: Some("rc_mcp".to_owned()),
            ..ErrorLogQuery::default()
        })
        .await
        .expect("read");
    assert_eq!(by_target.len(), 1);

    let one = db
        .error_log(&ErrorLogQuery {
            limit: Some(1),
            ..ErrorLogQuery::default()
        })
        .await
        .expect("read");
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].message, "listing failed");
}

#[tokio::test]
async fn clearing_removes_every_line() {
    let db = seeded().await;
    assert_eq!(db.clear_error_log().await.expect("clear"), 3);
    assert!(
        db.error_log(&ErrorLogQuery::default())
            .await
            .expect("read")
            .is_empty()
    );
    assert_eq!(db.clear_error_log().await.expect("clear again"), 0);
}

#[test]
fn levels_use_the_stored_spelling() {
    for level in [LogLevel::Warn, LogLevel::Error] {
        assert_eq!(LogLevel::from_stored(level.as_str()), level);
        assert_eq!(
            serde_json::to_value(level).expect("serializes"),
            level.as_str()
        );
    }
}
