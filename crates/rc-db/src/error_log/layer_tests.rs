use tracing_subscriber::layer::SubscriberExt as _;

use super::*;
use crate::{Db, ErrorLogQuery, diagnostics::store};

const SECRET: &str = "hunter2-planted-0815";

/// Runs `log` under a subscriber with only the error-log layer - no level filter in front of
/// it, so the layer's own rule is what is tested - and stores what it produced.
async fn logged(db: &Db, log: impl FnOnce()) {
    let (layer, mut receiver) = channel();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), log);
    let mut lines = Vec::new();
    while let Ok(line) = receiver.try_recv() {
        lines.push(line);
    }
    store(db, &mut lines).await;
}

async fn stored(db: &Db) -> Vec<crate::ErrorLogEntry> {
    db.error_log(&ErrorLogQuery::default()).await.expect("read")
}

#[tokio::test]
async fn info_and_debug_never_reach_the_database() {
    let db = Db::open_in_memory().await.expect("open");
    logged(&db, || {
        tracing::trace!("trace line");
        tracing::debug!("debug line");
        tracing::info!(path = "C:\\", "info line");
    })
    .await;
    assert!(stored(&db).await.is_empty());
}

#[tokio::test]
async fn warn_and_error_are_stored_with_target_and_code() {
    let db = Db::open_in_memory().await.expect("open");
    logged(&db, || {
        tracing::warn!(target: "rc_mcp::http", origin = "https://evil.example", "origin refused");
        tracing::error!(code = "not_found", path = "D:\\gone", "listing failed");
    })
    .await;
    let entries = stored(&db).await;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, LogLevel::Error);
    assert_eq!(entries[0].code.as_deref(), Some("not_found"));
    assert_eq!(entries[0].message, "listing failed path=D:\\gone");
    assert_eq!(entries[1].level, LogLevel::Warn);
    assert_eq!(entries[1].target, "rc_mcp::http");
    assert_eq!(
        entries[1].message,
        "origin refused origin=https://evil.example"
    );
}

#[tokio::test]
async fn a_secret_in_a_message_is_stored_masked() {
    let db = Db::open_in_memory().await.expect("open");
    logged(&db, || {
        tracing::error!("login to sftp://bob:{SECRET}@host.example failed");
        tracing::error!("retry with password={SECRET} refused");
        tracing::warn!(password = SECRET, token = %SECRET, "connect failed");
        tracing::warn!(header = %format!("Authorization: Bearer {SECRET}"), "proxy said no");
    })
    .await;
    let entries = stored(&db).await;
    assert_eq!(entries.len(), 4);
    for entry in &entries {
        assert!(!entry.message.contains(SECRET), "{}", entry.message);
        assert!(entry.message.contains(PLACEHOLDER) || entry.message.contains("redacted"));
    }
    assert!(
        entries
            .iter()
            .any(|entry| entry.message == "connect failed password=[redacted] token=[redacted]"),
        "{entries:#?}"
    );
}

#[tokio::test]
async fn a_long_message_is_capped_at_a_character_boundary() {
    let db = Db::open_in_memory().await.expect("open");
    let long = "\u{20ac}".repeat(MAX_MESSAGE_BYTES);
    logged(&db, || tracing::error!("{long}")).await;
    let entries = stored(&db).await;
    assert_eq!(entries.len(), 1);
    assert!(entries[0].message.len() <= MAX_MESSAGE_BYTES);
    assert!(entries[0].message.ends_with(CUT_MARK));
}

#[tokio::test]
async fn the_sinks_own_complaints_are_not_stored() {
    let db = Db::open_in_memory().await.expect("open");
    logged(&db, || {
        tracing::warn!(target: UNSTORED_TARGET, "error log lines could not be stored");
    })
    .await;
    assert!(stored(&db).await.is_empty());
}

#[test]
fn a_full_channel_drops_instead_of_blocking() {
    let (layer, receiver) = channel();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
        for index in 0..CHANNEL_CAPACITY + 10 {
            tracing::error!(index, "burst");
        }
    });
    assert_eq!(receiver.len(), CHANNEL_CAPACITY);
}

#[test]
fn cap_keeps_short_text_whole() {
    assert_eq!(cap("short".to_owned(), 10), "short");
    assert_eq!(cap("0123456789abc".to_owned(), 10), "0123456...");
}
