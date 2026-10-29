use rc_db::Db;
use serde_json::{Value, json};

use super::tests::{engine_with, logged, theme};
use super::*;
use crate::{Command, Outcome};

/// Every row read in one query comes to what it comes to read alone: the stored value, the
/// default for a bad or an unreadable one, the default for nothing stored (RC-080-50).
#[tokio::test]
async fn settings_get_in_one_query_reads_as_each_row_alone() {
    let db = Db::open_in_memory().await.expect("db");
    db.set(THEME_KEY, &42).await.expect("stored");
    db.set(SHELL_KEY, "bash").await.expect("stored");
    let (_dir, engine) = engine_with(db.clone()).await;
    let Outcome::Settings { settings } = engine
        .dispatch(Command::GetSettings { key: None })
        .await
        .expect("read")
    else {
        panic!("settings");
    };
    for read in &settings {
        let spec = setting(&read.key).expect("a row");
        assert_eq!(
            read.value,
            effective(spec, db.get::<Value>(spec.key).await),
            "{}",
            read.key
        );
    }

    let (value, warnings) = logged(|| effective(theme(), parsed(THEME_KEY, Some(&"{".to_owned()))));
    assert_eq!(value, json!("system"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(parsed(THEME_KEY, None).expect("nothing stored"), None);
}
