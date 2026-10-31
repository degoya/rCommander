use rc_db::Db;
use serde_json::{Value, json};

use super::{CUSTOM_DARK, CUSTOM_LIGHT, adopt_colours, mix};
use crate::settings::themes::{Theme, ThemeAccent, load};

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

async fn own(db: &Db) -> Vec<Theme> {
    db.themes()
        .await
        .expect("read")
        .into_iter()
        .filter_map(Theme::from_stored)
        .collect()
}

/// Nothing chosen: no theme is made, nothing is chosen, and the defaults stay in force.
#[tokio::test]
async fn empty_colours_make_no_theme() {
    let db = db().await;
    db.set("appearance.colours.light.text", &json!(""))
        .await
        .expect("set");
    let changed = adopt_colours(&db).await.expect("adopt");
    assert_eq!(changed, cfg!(target_os = "macos"), "only the Mac's look");
    assert!(own(&db).await.is_empty());
    if !cfg!(target_os = "macos") {
        let state = load(&db).await;
        assert_eq!(state.chosen, ["fluent-light", "fluent-dark"]);
    }
}

/// The dark look's colours become "Custom (dark)", chosen, with the text softened for the quieter
/// text; the keys are gone, so a second start changes nothing.
#[tokio::test]
async fn chosen_colours_become_a_chosen_custom_theme() {
    let db = db().await;
    db.set("appearance.colours.dark.text", &json!("#ffffff"))
        .await
        .expect("set");
    db.set("appearance.colours.dark.background", &json!("#000000"))
        .await
        .expect("set");
    db.set("appearance.colours.dark.accent", &json!("#FF8000"))
        .await
        .expect("set");
    assert!(adopt_colours(&db).await.expect("adopt"));

    let themes = own(&db).await;
    assert_eq!(themes.len(), 1, "the light look chose nothing");
    let custom = &themes[0];
    assert_eq!(custom.name, CUSTOM_DARK);
    assert!(custom.dark);
    assert_eq!(custom.accent, ThemeAccent::Colour("#ff8000".into()));
    let token = |name: &str| custom.tokens.get(name).map(String::as_str);
    assert_eq!(token("list_fill"), Some("#000000"));
    assert_eq!(token("list_text"), Some("#ffffff"));
    assert_eq!(token("cursor_text"), Some("#ffffff"));
    assert_eq!(token("list_text_secondary"), Some("#bfbfbf"));
    assert_eq!(token("window"), None, "the rest stays the fixed set's");

    let state = load(&db).await;
    assert_eq!(state.chosen[1], custom.id);
    for key in [
        "appearance.colours.dark.text",
        "appearance.colours.dark.background",
        "appearance.colours.dark.accent",
    ] {
        assert!(db.get::<Value>(key).await.expect("read").is_none(), "{key}");
    }
    if !cfg!(target_os = "macos") {
        assert!(!adopt_colours(&db).await.expect("again"));
    }
}

/// A colour set later - an import from Total Commander - lands in the same Custom theme.
#[tokio::test]
async fn a_later_colour_updates_the_custom_theme() {
    let db = db().await;
    db.set("appearance.colours.light.cursor", &json!("#123456"))
        .await
        .expect("set");
    adopt_colours(&db).await.expect("adopt");
    db.set("appearance.colours.light.marked", &json!("#c42b1c"))
        .await
        .expect("set");
    adopt_colours(&db).await.expect("adopt again");
    let themes = own(&db).await;
    assert_eq!(themes.len(), 1);
    assert_eq!(themes[0].name, CUSTOM_LIGHT);
    assert_eq!(
        themes[0].tokens.get("cursor_fill").map(String::as_str),
        Some("#123456")
    );
    assert_eq!(
        themes[0].tokens.get("marked").map(String::as_str),
        Some("#c42b1c")
    );
}

#[test]
fn mixing_weighs_the_top_colour() {
    assert_eq!(mix("#ffffff", "#000000", 0.75).as_deref(), Some("#bfbfbf"));
    assert_eq!(mix("#000000", "#ffffff", 1.0).as_deref(), Some("#000000"));
    assert_eq!(mix("#zzzzzz", "#000000", 0.5), None);
}
