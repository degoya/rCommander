use crate::{
    Db, StoredTheme,
    connections::{ConnectionDraft, ConnectionError, ConnectionPatch, MAX_THEME_ID_CHARS},
};

fn own(id: &str, dark: bool) -> StoredTheme {
    StoredTheme {
        id: id.to_owned(),
        name: id.to_owned(),
        dark,
        accent: String::new(),
        tokens: "{}".to_owned(),
        saved_at: String::new(),
    }
}

fn themed(name: &str, light: &str, dark: &str) -> ConnectionDraft {
    ConnectionDraft {
        theme_light: Some(light.to_owned()),
        theme_dark: Some(dark.to_owned()),
        ..ConnectionDraft::new(name, "example.org")
    }
}

#[tokio::test]
async fn the_themes_are_stored_changed_and_cleared() {
    let db = Db::open_in_memory().await.expect("open");
    let created = db
        .create_connection(None, &themed("web", " nord-light ", "nord-dark"))
        .await
        .expect("create");
    assert_eq!(
        created.theme_light.as_deref(),
        Some("nord-light"),
        "trimmed"
    );
    assert_eq!(created.theme_dark.as_deref(), Some("nord-dark"));
    let patch = ConnectionPatch {
        theme_dark: Some("solarized-dark".to_owned()),
        ..ConnectionPatch::default()
    };
    let changed = db
        .update_connection(created.id, &patch)
        .await
        .expect("dark");
    assert_eq!(changed.theme_light.as_deref(), Some("nord-light"), "kept");
    assert_eq!(changed.theme_dark.as_deref(), Some("solarized-dark"));
    let clear = ConnectionPatch {
        theme_light: Some(String::new()),
        ..ConnectionPatch::default()
    };
    let cleared = db
        .update_connection(created.id, &clear)
        .await
        .expect("clear");
    assert_eq!(cleared.theme_light, None, "empty clears");
    let listed = db.connection_themes().await.expect("themes");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
    assert_eq!(listed[0].light, None);
    assert_eq!(listed[0].dark.as_deref(), Some("solarized-dark"));
}

#[tokio::test]
async fn a_connection_without_themes_is_not_listed() {
    let db = Db::open_in_memory().await.expect("open");
    db.create_connection(None, &ConnectionDraft::new("plain", "example.org"))
        .await
        .expect("create");
    assert!(db.connection_themes().await.expect("themes").is_empty());
}

#[tokio::test]
async fn a_deleted_own_theme_reads_back_as_none() {
    let db = Db::open_in_memory().await.expect("open");
    db.save_theme(&own("own-day", false)).await.expect("day");
    db.save_theme(&own("own-night", true)).await.expect("night");
    let created = db
        .create_connection(None, &themed("web", "own-day", "own-night"))
        .await
        .expect("create");
    assert_eq!(created.theme_light.as_deref(), Some("own-day"));
    assert!(db.delete_theme("own-night").await.expect("delete"));
    let read = db.connection(created.id).await.expect("read");
    assert_eq!(read.theme_light.as_deref(), Some("own-day"));
    assert_eq!(read.theme_dark, None, "the deleted theme falls back");
    let listed = db.connection_themes().await.expect("themes");
    assert_eq!(listed[0].dark, None);
    let tree = db.connection_tree().await.expect("tree");
    assert_eq!(tree.connections[0].theme_dark, None);
}

#[test]
fn a_patch_of_the_themes_alone_says_so() {
    let themes = ConnectionPatch {
        theme_light: Some("nord-light".to_owned()),
        ..ConnectionPatch::default()
    };
    assert!(themes.only_themes());
    assert!(ConnectionPatch::default().only_themes());
    let host = ConnectionPatch {
        host: Some("other.org".to_owned()),
        ..themes
    };
    assert!(!host.only_themes());
}

#[tokio::test]
async fn a_theme_id_too_long_or_with_a_control_character_is_refused() {
    let db = Db::open_in_memory().await.expect("open");
    let long = "a".repeat(MAX_THEME_ID_CHARS + 1);
    let refused = db
        .create_connection(None, &themed("long", &long, ""))
        .await
        .expect_err("too long");
    assert!(matches!(
        refused,
        ConnectionError::TextTooLong { field: "theme", .. }
    ));
    let refused = db
        .create_connection(None, &themed("ctl", "nord\u{1b}-light", ""))
        .await
        .expect_err("control character");
    assert!(matches!(
        refused,
        ConnectionError::ControlCharacter { field: "theme" }
    ));
}
