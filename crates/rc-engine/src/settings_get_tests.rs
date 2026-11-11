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

/// RC-110-15: `buttonbar.icon_size` was a choice of "16", "24" and "32" until 0.21; stored as that
/// text it reads as the number, a number in 16 to 48 reads as itself, and text that names no
/// number or one out of bounds falls back to 24.
#[tokio::test]
async fn the_old_icon_sizes_read_as_before() {
    let db = Db::open_in_memory().await.expect("db");
    for (stored, read) in [
        (json!("16"), json!(16)),
        (json!("24"), json!(24)),
        (json!("32"), json!(32)),
        (json!(40), json!(40)),
        (json!(48), json!(48)),
        (json!("big"), json!(24)),
        (json!(64), json!(24)),
        (json!("8"), json!(24)),
    ] {
        db.set(ICON_SIZE_KEY, &stored).await.expect("stored");
        assert_eq!(
            stored_setting(&db, ICON_SIZE_KEY).await,
            Some(read),
            "{stored}"
        );
    }
    let spec = setting(ICON_SIZE_KEY).expect("a row");
    assert_eq!(number_of_text(spec, json!("32")), json!(32));
    // Only an `Int` row reads text as a number: a choice keeps its text.
    assert_eq!(number_of_text(theme(), json!("32")), json!("32"));
    assert_eq!(
        setting(FLAT_KEY).map(SettingSpec::default_value),
        Some(json!(false))
    );
    assert_eq!(
        setting(BUTTON_PADDING_KEY).map(SettingSpec::default_value),
        Some(json!(DEFAULT_BUTTON_PADDING))
    );
}
