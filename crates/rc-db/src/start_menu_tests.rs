use crate::{
    CommandFields, Db, MAX_START_MENU_ITEMS, START_MENU_MAX_DEPTH, StartMenuRecord,
    UserCommandStoreError,
};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

fn entry(title: &str) -> StartMenuRecord {
    StartMenuRecord {
        kind: "item".to_owned(),
        title: title.to_owned(),
        fields: CommandFields {
            command: "regedit.exe".to_owned(),
            elevated: true,
            ..CommandFields::default()
        },
    }
}

fn submenu(title: &str) -> StartMenuRecord {
    StartMenuRecord {
        kind: "submenu".to_owned(),
        title: title.to_owned(),
        fields: CommandFields::default(),
    }
}

async fn titles(db: &Db) -> Vec<(Option<i64>, String)> {
    db.start_menu()
        .await
        .expect("read")
        .into_iter()
        .map(|item| (item.parent_id, item.record.title))
        .collect()
}

#[tokio::test]
async fn rows_are_added_in_order_changed_moved_and_deleted() {
    let db = db().await;
    let tools = db
        .add_start_menu_item(None, &submenu("Tools"))
        .await
        .expect("add");
    let regedit = db
        .add_start_menu_item(Some(tools.id), &entry("Registry"))
        .await
        .expect("add");
    let notes = db
        .add_start_menu_item(None, &entry("Notes"))
        .await
        .expect("add");
    assert_eq!(regedit.position, 0);
    assert_eq!(notes.position, 1);
    assert!(regedit.record.fields.elevated);
    assert_eq!(
        titles(&db).await,
        vec![
            (None, "Tools".to_owned()),
            (None, "Notes".to_owned()),
            (Some(tools.id), "Registry".to_owned()),
        ]
    );

    let renamed = db
        .update_start_menu_item(notes.id, &entry("Editor"))
        .await
        .expect("update");
    assert_eq!(renamed.position, 1);
    assert_eq!(renamed.record.title, "Editor");

    let moved = db
        .move_start_menu_item(notes.id, Some(tools.id), Some(0))
        .await
        .expect("move");
    assert_eq!(moved.parent_id, Some(tools.id));
    assert_eq!(moved.position, 0);

    assert_eq!(
        db.delete_start_menu_item(tools.id).await.expect("delete"),
        3
    );
    assert!(db.start_menu().await.expect("read").is_empty());
}

#[tokio::test]
async fn only_a_submenu_holds_rows_and_never_itself() {
    let db = db().await;
    let notes = db
        .add_start_menu_item(None, &entry("Notes"))
        .await
        .expect("add");
    let error = db
        .add_start_menu_item(Some(notes.id), &entry("Inside"))
        .await
        .expect_err("no submenu");
    assert!(matches!(error, UserCommandStoreError::NotASubmenu { .. }));

    let outer = db
        .add_start_menu_item(None, &submenu("Outer"))
        .await
        .expect("add");
    let inner = db
        .add_start_menu_item(Some(outer.id), &submenu("Inner"))
        .await
        .expect("add");
    let error = db
        .move_start_menu_item(outer.id, Some(inner.id), None)
        .await
        .expect_err("a cycle");
    assert!(matches!(error, UserCommandStoreError::Cycle));

    // A submenu with rows stays a submenu.
    let error = db
        .update_start_menu_item(outer.id, &entry("Outer"))
        .await
        .expect_err("not empty");
    assert!(matches!(error, UserCommandStoreError::NotEmpty { .. }));
}

#[tokio::test]
async fn the_menu_nests_at_most_8_levels() {
    let db = db().await;
    let mut parent = None;
    for level in 0..START_MENU_MAX_DEPTH {
        let added = db
            .add_start_menu_item(parent, &submenu(&format!("Level {level}")))
            .await
            .expect("fits");
        parent = Some(added.id);
    }
    let error = db
        .add_start_menu_item(parent, &entry("Too deep"))
        .await
        .expect_err("too deep");
    assert!(
        matches!(error, UserCommandStoreError::TooDeep { max: 8 }),
        "{error:?}"
    );
    // A subtree two levels high does not fit under the seventh level either.
    let top = db
        .add_start_menu_item(None, &submenu("Top"))
        .await
        .expect("add");
    db.add_start_menu_item(Some(top.id), &entry("Leaf"))
        .await
        .expect("add");
    let first = db.start_menu().await.expect("read")[0].id;
    let seventh = {
        let mut at = first;
        for _ in 1..7 {
            at = db
                .start_menu()
                .await
                .expect("read")
                .into_iter()
                .find(|item| item.parent_id == Some(at))
                .expect("a child")
                .id;
        }
        at
    };
    let error = db
        .move_start_menu_item(top.id, Some(seventh), None)
        .await
        .expect_err("too deep");
    assert!(matches!(error, UserCommandStoreError::TooDeep { .. }));
}

#[tokio::test]
async fn at_most_512_rows_are_kept() {
    let db = db().await;
    for at in 0..MAX_START_MENU_ITEMS {
        db.add_start_menu_item(None, &entry(&format!("Entry {at}")))
            .await
            .expect("fits");
    }
    let error = db
        .add_start_menu_item(None, &entry("One more"))
        .await
        .expect_err("full");
    assert!(
        matches!(error, UserCommandStoreError::TooMany { max: 512 }),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_title_or_command_too_long_is_refused() {
    let db = db().await;
    let mut long = entry("Long");
    long.title = "t".repeat(4097);
    assert!(db.add_start_menu_item(None, &long).await.is_err());
    let mut long = entry("Long");
    long.fields.command = "c".repeat(4097);
    assert!(db.add_start_menu_item(None, &long).await.is_err());
    let mut wrong = entry("Wrong");
    wrong.kind = "folder".to_owned();
    assert!(db.add_start_menu_item(None, &wrong).await.is_err());
}
