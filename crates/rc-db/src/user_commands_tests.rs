use crate::{
    CommandFields, DURABLE_TABLES, Db, MAX_USER_COMMAND_FIELD, MAX_USER_COMMANDS,
    UserCommandRecord, UserCommandStoreError,
};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

fn record(name: &str) -> UserCommandRecord {
    UserCommandRecord {
        name: name.to_owned(),
        fields: CommandFields {
            command: "notepad.exe".to_owned(),
            params: "\"%P%N\"".to_owned(),
            ..CommandFields::default()
        },
        tooltip: "Notepad".to_owned(),
        ..UserCommandRecord::default()
    }
}

#[test]
fn both_tables_are_durable() {
    for table in ["user_commands", "start_menu_items"] {
        assert!(
            DURABLE_TABLES.iter().any(|(name, _)| *name == table),
            "{table}: a person makes and removes them"
        );
    }
}

#[tokio::test]
async fn an_own_command_is_stored_read_changed_and_deleted() {
    let db = db().await;
    assert!(db.user_commands().await.expect("read").is_empty());
    let made = db
        .save_user_command(None, &record("em_np"))
        .await
        .expect("save");
    assert_eq!(made.record, record("em_np"));
    let found = db
        .user_command_named("EM_NP")
        .await
        .expect("read")
        .expect("found without regard to case");
    assert_eq!(found.id, made.id);

    let mut changed = record("em_editor");
    changed.fields.elevated = true;
    changed.fields.show = "maximized".to_owned();
    let saved = db
        .save_user_command(Some(made.id), &changed)
        .await
        .expect("update");
    assert_eq!(saved.id, made.id);
    assert_eq!(saved.record, changed);
    assert!(
        db.user_command_named("em_np")
            .await
            .expect("read")
            .is_none()
    );

    assert!(db.delete_user_command(made.id).await.expect("delete"));
    assert!(!db.delete_user_command(made.id).await.expect("delete"));
    assert!(db.user_commands().await.expect("read").is_empty());
}

#[tokio::test]
async fn a_name_is_taken_once_without_regard_to_case() {
    let db = db().await;
    db.save_user_command(None, &record("em_np"))
        .await
        .expect("save");
    let error = db
        .save_user_command(None, &record("em_NP"))
        .await
        .expect_err("taken");
    assert!(error.is_unique_violation(), "{error:?}");
}

#[tokio::test]
async fn the_table_refuses_what_the_checks_name() {
    let db = db().await;
    for name in ["np", "em_", "xx_np"] {
        assert!(
            db.save_user_command(None, &record(name)).await.is_err(),
            "{name}"
        );
    }
    let mut wrong_kind = record("em_kind");
    wrong_kind.fields.kind = "script".to_owned();
    assert!(db.save_user_command(None, &wrong_kind).await.is_err());
    let mut wrong_show = record("em_show");
    wrong_show.fields.show = "hidden".to_owned();
    assert!(db.save_user_command(None, &wrong_show).await.is_err());
    assert!(
        db.save_user_command(Some(99), &record("em_none"))
            .await
            .is_err_and(|error| matches!(error, UserCommandStoreError::NotFound { id: 99 }))
    );
}

#[tokio::test]
async fn a_field_holds_at_most_4096_characters() {
    let db = db().await;
    let mut full = record("em_full");
    full.fields.params = "x".repeat(MAX_USER_COMMAND_FIELD);
    db.save_user_command(None, &full).await.expect("fits");
    let mut long = record("em_long");
    long.fields.params = "x".repeat(MAX_USER_COMMAND_FIELD + 1);
    let error = db
        .save_user_command(None, &long)
        .await
        .expect_err("too long");
    assert!(
        matches!(
            error,
            UserCommandStoreError::Field {
                field: "params",
                ..
            }
        ),
        "{error:?}"
    );
    let mut tooltip = record("em_tip");
    tooltip.tooltip = "y".repeat(MAX_USER_COMMAND_FIELD + 1);
    assert!(db.save_user_command(None, &tooltip).await.is_err());
}

#[tokio::test]
async fn at_most_512_own_commands_are_kept() {
    let db = db().await;
    for at in 0..MAX_USER_COMMANDS {
        db.save_user_command(None, &record(&format!("em_c{at}")))
            .await
            .expect("fits");
    }
    let error = db
        .save_user_command(None, &record("em_one_more"))
        .await
        .expect_err("full");
    assert!(
        matches!(error, UserCommandStoreError::TooMany { max: 512 }),
        "{error:?}"
    );
    // A change of a kept one still goes through.
    let first = db
        .user_command_named("em_c0")
        .await
        .expect("read")
        .expect("there");
    db.save_user_command(Some(first.id), &record("em_c0"))
        .await
        .expect("update");
}
