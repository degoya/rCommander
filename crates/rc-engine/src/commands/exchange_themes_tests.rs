use std::collections::BTreeMap;

use super::export_themes;
use crate::{
    Command,
    exchange::{ExportedTheme, RowStatus},
    settings::themes::ThemeDraft,
    test_support,
};

fn draft(name: &str) -> ThemeDraft {
    ThemeDraft {
        id: None,
        name: name.to_owned(),
        dark: true,
        accent: "#ff8000".to_owned(),
        tokens: BTreeMap::from([("window".to_owned(), "#101010".to_owned())]),
    }
}

/// An own theme goes out with its id and comes into a fresh database as new; a second import
/// finds it there.
#[tokio::test]
async fn an_own_theme_goes_out_and_comes_in_once() {
    let (_root, source) = test_support::engine_with_db(&[]).await;
    source
        .dispatch(Command::SaveTheme {
            theme: draft("Night"),
        })
        .await
        .expect("save");
    let exported = export_themes(source.db().expect("db")).await;
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0].accent, "#ff8000");

    let (_other, target) = test_support::engine_with_db(&[]).await;
    let rows = target
        .exchange_themes(&exported, None, false)
        .await
        .expect("import");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, RowStatus::Created);
    let stored = target
        .db()
        .expect("db")
        .theme(&exported[0].id)
        .await
        .expect("read")
        .expect("there");
    assert_eq!(stored.name, "Night");

    let again = target
        .exchange_themes(&exported, None, false)
        .await
        .expect("again");
    assert_eq!(again[0].status, RowStatus::Exists);
}

/// A theme with a colour that is none, or an id that is no own theme's, is not imported; a name
/// taken here gets a number.
#[tokio::test]
async fn a_bad_theme_is_refused_and_a_taken_name_numbered() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    engine
        .dispatch(Command::SaveTheme {
            theme: draft("Night"),
        })
        .await
        .expect("save");
    let theme = |id: &str, colour: &str| ExportedTheme {
        id: id.to_owned(),
        name: "Night".to_owned(),
        dark: true,
        accent: "system".to_owned(),
        tokens: BTreeMap::from([("window".to_owned(), colour.to_owned())]),
    };
    let rows = engine
        .exchange_themes(
            &[
                theme("own-aaaa0001", "white"),
                theme("fluent-dark", "#000000"),
                theme("own-aaaa0002", "#000000"),
            ],
            None,
            false,
        )
        .await
        .expect("import");
    assert!(matches!(rows[0].status, RowStatus::Refused { .. }));
    assert!(matches!(rows[1].status, RowStatus::Refused { .. }));
    assert_eq!(rows[2].status, RowStatus::Created);
    let stored = engine
        .db()
        .expect("db")
        .theme("own-aaaa0002")
        .await
        .expect("read")
        .expect("there");
    assert_eq!(stored.name, "Night (2)");
}

/// RC-100-56: the WSL distributions' themes come in with the settings; a theme that is neither
/// here nor in the file is left out with a hint, one the file brings counts in a dry run.
#[tokio::test]
async fn a_distribution_s_missing_theme_is_left_out_with_a_hint() {
    use crate::{exchange::ExportedSetting, settings::WSL_THEMES_KEY};

    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let setting = ExportedSetting {
        setting: WSL_THEMES_KEY.to_owned(),
        value: serde_json::Value::from(
            "Ubuntu\tnord-light\town-aaaa0003\nDebian\town-gone\tnord-dark",
        ),
    };
    let night = ExportedTheme {
        id: "own-aaaa0003".to_owned(),
        name: "Night".to_owned(),
        dark: true,
        accent: "system".to_owned(),
        tokens: BTreeMap::new(),
    };
    let dry = engine
        .exchange_settings(std::slice::from_ref(&setting), &[night], None, true)
        .await
        .expect("dry run");
    assert_eq!(dry[0].warning.as_deref(), Some("theme_missing"));
    assert!(dry[0].selected, "only a hint: the row stays chosen");
    assert_eq!(
        dry[0].new.as_deref(),
        Some("Ubuntu\tnord-light\town-aaaa0003\nDebian\t\tnord-dark"),
        "the file's own theme counts, the missing one is left out"
    );

    let written = engine
        .exchange_settings(std::slice::from_ref(&setting), &[], None, false)
        .await
        .expect("import");
    assert_eq!(written[0].status, RowStatus::Updated);
    let stored = crate::settings::stored_setting(engine.db().expect("db"), WSL_THEMES_KEY).await;
    assert_eq!(
        stored.as_ref().and_then(serde_json::Value::as_str),
        Some("Ubuntu\tnord-light\t\nDebian\t\tnord-dark"),
        "without the file's theme written, its id is left out too"
    );
}
