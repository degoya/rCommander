use serde::{Deserialize, Serialize};

use crate::{Db, DbError};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Columns {
    widths: Vec<u16>,
    sort: String,
    descending: bool,
}

#[tokio::test]
async fn a_typed_value_round_trips() {
    let db = Db::open_in_memory().await.expect("open");
    let columns = Columns {
        widths: vec![240, 80, 120],
        sort: "name".to_owned(),
        descending: false,
    };
    db.set("view.columns", &columns).await.expect("set");
    assert_eq!(
        db.get::<Columns>("view.columns").await.expect("get"),
        Some(columns)
    );
}

#[tokio::test]
async fn a_missing_key_is_none() {
    let db = Db::open_in_memory().await.expect("open");
    assert_eq!(db.get::<bool>("never.set").await.expect("get"), None);
}

#[tokio::test]
async fn set_replaces_and_touches_updated_at() {
    let db = Db::open_in_memory().await.expect("open");
    db.set("theme", "dark").await.expect("set");
    db.set("theme", "light").await.expect("replace");
    assert_eq!(
        db.get::<String>("theme").await.expect("get"),
        Some("light".to_owned())
    );
    let (rows, stamp): (i64, String) =
        sqlx::query_as("SELECT COUNT(*), MAX(updated_at) FROM settings")
            .fetch_one(db.readers())
            .await
            .expect("row");
    assert_eq!(rows, 1);
    assert!(stamp.ends_with('Z') && stamp.contains('T'), "{stamp}");
}

#[tokio::test]
async fn delete_says_whether_there_was_a_value() {
    let db = Db::open_in_memory().await.expect("open");
    db.set("hotlist", &["C:\\", "D:\\"]).await.expect("set");
    assert!(db.delete("hotlist").await.expect("delete"));
    assert!(!db.delete("hotlist").await.expect("delete again"));
    assert_eq!(db.get::<Vec<String>>("hotlist").await.expect("get"), None);
}

#[tokio::test]
async fn a_value_of_another_type_is_a_json_error_naming_the_key() {
    let db = Db::open_in_memory().await.expect("open");
    db.set("font.size", "large").await.expect("set");
    let error = db.get::<u32>("font.size").await.expect_err("wrong type");
    assert!(
        matches!(&error, DbError::Json { key, .. } if key == "font.size"),
        "{error}"
    );
}

#[tokio::test]
async fn concurrent_writers_take_turns() {
    let db = Db::open_in_memory().await.expect("open");
    let writes = (0..32).map(|index| {
        let db = db.clone();
        tokio::spawn(async move { db.set(&format!("key.{index}"), &index).await })
    });
    for write in writes {
        write.await.expect("task").expect("set");
    }
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
        .fetch_one(db.readers())
        .await
        .expect("count");
    assert_eq!(rows, 32);
}

#[tokio::test]
async fn several_keys_are_read_at_once_as_stored_text() {
    let db = Db::open_in_memory().await.expect("open");
    db.set("a.one", &1_u32).await.expect("set");
    db.set("a.two", "two").await.expect("set");
    db.set("b.other", &true).await.expect("set");
    let read = db
        .get_json_many(&["a.one", "a.two", "never.set"])
        .await
        .expect("read");
    assert_eq!(
        read.into_iter().collect::<Vec<_>>(),
        [
            ("a.one".to_owned(), "1".to_owned()),
            ("a.two".to_owned(), "\"two\"".to_owned())
        ]
    );
    assert!(db.get_json_many(&[]).await.expect("read").is_empty());
}
