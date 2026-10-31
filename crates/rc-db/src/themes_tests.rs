use crate::{DURABLE_TABLES, Db, StoredTheme};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

fn theme(id: &str, name: &str, dark: bool) -> StoredTheme {
    StoredTheme {
        id: id.to_owned(),
        name: name.to_owned(),
        dark,
        accent: String::new(),
        tokens: "{\"window\":\"#101010\"}".to_owned(),
        saved_at: String::new(),
    }
}

#[tokio::test]
async fn a_theme_is_saved_read_back_replaced_and_deleted() {
    let db = db().await;
    assert!(db.themes().await.expect("read").is_empty());
    let saved = db
        .save_theme(&theme("own-1", "Night", true))
        .await
        .expect("save");
    assert_eq!((saved.id.as_str(), saved.name.as_str()), ("own-1", "Night"));
    assert!(saved.dark);
    assert!(saved.saved_at.ends_with('Z'));
    db.save_theme(&theme("own-2", "Day", false))
        .await
        .expect("save");
    let names: Vec<String> = db
        .themes()
        .await
        .expect("read")
        .into_iter()
        .map(|theme| theme.name)
        .collect();
    assert_eq!(names, ["Day", "Night"], "the light ones first");

    let mut renamed = theme("own-1", "Midnight", true);
    renamed.accent = "#ff8000".to_owned();
    db.save_theme(&renamed).await.expect("replace");
    let read = db.theme("own-1").await.expect("read").expect("there");
    assert_eq!(read.name, "Midnight");
    assert_eq!(read.accent, "#ff8000");
    assert_eq!(
        db.themes().await.expect("read").len(),
        2,
        "replaced, not added"
    );

    assert!(db.delete_theme("own-1").await.expect("delete"));
    assert!(!db.delete_theme("own-1").await.expect("delete again"));
    assert!(db.theme("own-1").await.expect("read").is_none());
}

/// Two themes never share a name, whatever its case; tokens that are no JSON are refused.
#[tokio::test]
async fn names_are_unique_and_tokens_are_json() {
    let db = db().await;
    db.save_theme(&theme("own-1", "Night", true))
        .await
        .expect("save");
    assert!(
        db.save_theme(&theme("own-2", "NIGHT", false))
            .await
            .is_err()
    );
    let mut broken = theme("own-3", "Broken", false);
    broken.tokens = "{not json".to_owned();
    assert!(db.save_theme(&broken).await.is_err());
    assert!(db.save_theme(&theme("own-4", "  ", false)).await.is_err());
    assert_eq!(db.themes().await.expect("read").len(), 1);
}

#[test]
fn the_table_is_durable() {
    assert!(DURABLE_TABLES.iter().any(|(name, _)| *name == "themes"));
}
