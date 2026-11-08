use crate::{
    ButtonBarStoreError, DURABLE_TABLES, Db, MAX_BUTTON_BAR_NAME, MAX_BUTTON_BAR_SPEC,
    MAX_BUTTON_BARS,
};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

const EMPTY: &str = r#"{"version":1,"items":[]}"#;

#[test]
fn the_table_is_durable() {
    assert!(
        DURABLE_TABLES
            .iter()
            .any(|(name, _)| *name == "button_bars"),
        "a person makes and removes the bars"
    );
}

#[tokio::test]
async fn the_first_start_has_the_bar_default() {
    let db = db().await;
    let bars = db.button_bars().await.expect("read");
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].name, "Default");
    assert!(bars[0].spec.contains("panel.refresh"));
}

#[tokio::test]
async fn a_bar_is_stored_read_changed_renamed_and_deleted() {
    let db = db().await;
    let made = db
        .save_button_bar(None, "Tools", EMPTY)
        .await
        .expect("save");
    assert_eq!(made.name, "Tools");
    assert_eq!(made.spec, EMPTY);
    let read = db.button_bar(made.id).await.expect("read").expect("there");
    assert_eq!(read, made);

    let spec = r#"{"version":1,"items":[{"type":"separator"}]}"#;
    let changed = db
        .save_button_bar(Some(made.id), "Tools", spec)
        .await
        .expect("change");
    assert_eq!(changed.id, made.id);
    assert_eq!(changed.spec, spec);

    let renamed = db
        .rename_button_bar(made.id, "More tools")
        .await
        .expect("rename");
    assert_eq!(renamed.name, "More tools");

    assert!(db.delete_button_bar(made.id).await.expect("delete"));
    assert!(!db.delete_button_bar(made.id).await.expect("delete again"));
    assert!(db.button_bar(made.id).await.expect("read").is_none());
}

#[tokio::test]
async fn a_name_is_taken_without_regard_to_case() {
    let db = db().await;
    let error = db
        .save_button_bar(None, "DEFAULT", EMPTY)
        .await
        .expect_err("taken");
    assert!(error.is_unique_violation(), "{error}");
}

#[tokio::test]
async fn names_and_specs_that_do_not_fit_are_refused() {
    let db = db().await;
    for name in ["", "tab\there", &"x".repeat(MAX_BUTTON_BAR_NAME + 1)] {
        let error = db
            .save_button_bar(None, name, EMPTY)
            .await
            .expect_err("no name");
        assert!(matches!(error, ButtonBarStoreError::Name { .. }), "{error}");
    }
    let large = format!("{{\"pad\":\"{}\"}}", "x".repeat(MAX_BUTTON_BAR_SPEC));
    let error = db
        .save_button_bar(None, "Large", &large)
        .await
        .expect_err("too large");
    assert!(matches!(error, ButtonBarStoreError::SpecTooLarge { .. }));
    let error = db.rename_button_bar(1, "").await.expect_err("no name");
    assert!(matches!(error, ButtonBarStoreError::Name { .. }));
}

#[tokio::test]
async fn an_unknown_bar_is_not_found() {
    let db = db().await;
    let error = db
        .save_button_bar(Some(999), "Gone", EMPTY)
        .await
        .expect_err("not there");
    assert!(matches!(error, ButtonBarStoreError::NotFound { id: 999 }));
    let error = db
        .rename_button_bar(999, "Gone")
        .await
        .expect_err("not there");
    assert!(matches!(error, ButtonBarStoreError::NotFound { id: 999 }));
}

#[tokio::test]
async fn at_most_64_bars_are_kept() {
    let db = db().await;
    // The migration made "Default".
    for index in 1..MAX_BUTTON_BARS {
        db.save_button_bar(None, &format!("Bar {index}"), EMPTY)
            .await
            .expect("room");
    }
    let error = db
        .save_button_bar(None, "One too many", EMPTY)
        .await
        .expect_err("full");
    assert!(matches!(
        error,
        ButtonBarStoreError::TooMany {
            max: MAX_BUTTON_BARS
        }
    ));
}
