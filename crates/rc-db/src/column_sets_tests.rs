use crate::{DURABLE_TABLES, Db};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

const SPEC: &str = "{\"version\":1}";

async fn names(db: &Db) -> Vec<String> {
    db.column_sets()
        .await
        .expect("read")
        .into_iter()
        .map(|set| set.name)
        .collect()
}

#[test]
fn the_table_is_durable() {
    assert!(
        DURABLE_TABLES
            .iter()
            .any(|(table, _)| *table == "column_sets"),
        "a person makes and removes column sets"
    );
}

#[tokio::test]
async fn a_column_set_is_created_read_changed_and_deleted() {
    let db = db().await;
    assert!(db.column_sets().await.expect("read").is_empty());
    let made = db.create_column_set("Photos", SPEC).await.expect("create");
    assert_eq!(made.name, "Photos");
    assert_eq!(made.position, 0);
    assert_eq!(
        db.column_set(made.id).await.expect("read"),
        Some(made.clone())
    );

    let changed = db
        .update_column_set(
            made.id,
            Some("Pictures"),
            Some("{\"version\":1,\"columns\":[]}"),
        )
        .await
        .expect("update")
        .expect("there");
    assert_eq!(changed.name, "Pictures");
    assert_eq!(changed.spec, "{\"version\":1,\"columns\":[]}");
    let untouched = db
        .update_column_set(made.id, None, None)
        .await
        .expect("update")
        .expect("there");
    assert_eq!(untouched, changed, "no name and no spec change nothing");

    assert!(db.delete_column_set(made.id).await.expect("delete"));
    assert!(!db.delete_column_set(made.id).await.expect("gone already"));
    assert_eq!(db.column_set(made.id).await.expect("read"), None);
    assert_eq!(
        db.update_column_set(made.id, Some("x"), None)
            .await
            .expect("update"),
        None
    );
}

#[tokio::test]
async fn names_are_unique_without_regard_to_case_and_bounded() {
    let db = db().await;
    db.create_column_set("Photos", SPEC).await.expect("create");
    assert!(db.create_column_set("PHOTOS", SPEC).await.is_err());
    assert!(db.create_column_set("  ", SPEC).await.is_err());
    assert!(db.create_column_set(&"x".repeat(65), SPEC).await.is_err());
    assert!(db.create_column_set("Bad", "not json").await.is_err());
    let long = format!("{{\"pad\":\"{}\"}}", "x".repeat(16_384));
    assert!(db.create_column_set("Big", &long).await.is_err());
    assert_eq!(names(&db).await, ["Photos"]);
}

#[tokio::test]
async fn a_set_moves_and_the_list_is_numbered_anew() {
    let db = db().await;
    let a = db.create_column_set("A", SPEC).await.expect("a");
    db.create_column_set("B", SPEC).await.expect("b");
    db.create_column_set("C", SPEC).await.expect("c");
    assert!(db.move_column_set(a.id, 9).await.expect("move"));
    assert_eq!(names(&db).await, ["B", "C", "A"]);
    let positions: Vec<i64> = db
        .column_sets()
        .await
        .expect("read")
        .iter()
        .map(|set| set.position)
        .collect();
    assert_eq!(positions, [0, 1, 2]);
    assert!(!db.move_column_set(999, 0).await.expect("unknown"));
}
