use std::collections::BTreeMap;

use crate::{
    Command, EngineError, EngineEvent, Outcome,
    settings::themes::{ThemeDraft, ThemeMode, ThemesOutcome},
    test_support,
};

fn themes(outcome: Outcome) -> ThemesOutcome {
    match outcome {
        Outcome::Themes(themes) => *themes,
        other => panic!("not themes: {other:?}"),
    }
}

fn draft(name: &str, dark: bool) -> ThemeDraft {
    let mut tokens = BTreeMap::new();
    tokens.insert("window".to_owned(), "#101010".to_owned());
    tokens.insert("terminal.background".to_owned(), "#000000".to_owned());
    ThemeDraft {
        id: None,
        name: name.to_owned(),
        dark,
        accent: "system".to_owned(),
        tokens,
    }
}

#[tokio::test]
async fn the_list_offers_the_builtin_themes_with_fluent_chosen() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let list = themes(engine.dispatch(Command::ListThemes {}).await.expect("list"));
    let selected: Vec<&str> = list
        .themes
        .iter()
        .filter(|theme| theme.selected)
        .map(|theme| theme.id.as_str())
        .collect();
    assert_eq!(selected, ["fluent-light", "fluent-dark"]);
    for id in [
        "classic-dark",
        "high-contrast-light",
        "solarized-dark",
        "nord-light",
    ] {
        let row = list.themes.iter().find(|theme| theme.id == id);
        assert!(row.is_some_and(|row| row.builtin), "{id}");
    }
    let mac = list.themes.iter().any(|theme| theme.id.starts_with("mac-"));
    assert_eq!(mac, cfg!(target_os = "macos"));
}

/// `theme.select` over `Engine::dispatch`: the setting changes and is announced; a theme of the
/// other mode or none at all is refused.
#[tokio::test]
async fn select_stores_the_mode_and_refuses_the_wrong_one() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let mut events = engine.subscribe();
    let select = |mode, id: &str| Command::SelectTheme {
        mode,
        id: id.to_owned(),
    };
    let list = themes(
        engine
            .dispatch(select(ThemeMode::Dark, "nord-dark"))
            .await
            .expect("select"),
    );
    let nord = list.themes.iter().find(|theme| theme.id == "nord-dark");
    assert!(nord.is_some_and(|theme| theme.selected));
    let announced = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(event, EngineEvent::SettingChanged { key } if key == "appearance.theme_dark")
    });
    assert!(announced);

    let wrong = engine.dispatch(select(ThemeMode::Light, "nord-dark")).await;
    assert!(matches!(wrong, Err(EngineError::ThemeWrongMode { .. })));
    let unknown = engine.dispatch(select(ThemeMode::Light, "neon")).await;
    assert!(matches!(unknown, Err(EngineError::ThemeUnknown { .. })));
    let code = unknown.err().map(|error| error.code());
    assert_eq!(code, Some("theme_unknown"));
}

#[tokio::test]
async fn an_own_theme_is_saved_chosen_and_kept_from_deletion_while_chosen() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let saved = themes(
        engine
            .dispatch(Command::SaveTheme {
                theme: draft("Night", true),
            })
            .await
            .expect("save"),
    );
    let id = saved.saved.expect("its id");
    assert!(id.starts_with("own-"));
    engine
        .dispatch(Command::SelectTheme {
            mode: ThemeMode::Dark,
            id: id.clone(),
        })
        .await
        .expect("select");
    let in_use = engine
        .dispatch(Command::DeleteTheme { id: id.clone() })
        .await;
    assert!(matches!(in_use, Err(EngineError::ThemeInUse { .. })));

    engine
        .dispatch(Command::SelectTheme {
            mode: ThemeMode::Dark,
            id: "fluent-dark".into(),
        })
        .await
        .expect("select another");
    let after = themes(
        engine
            .dispatch(Command::DeleteTheme { id: id.clone() })
            .await
            .expect("delete"),
    );
    assert!(!after.themes.iter().any(|theme| theme.id == id));
}

#[tokio::test]
async fn a_save_checks_name_tokens_and_builtins() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let save = |theme| Command::SaveTheme { theme };
    engine
        .dispatch(save(draft("Night", true)))
        .await
        .expect("save");
    let taken = engine.dispatch(save(draft("NIGHT", false))).await;
    assert!(matches!(taken, Err(EngineError::ThemeNameTaken { .. })));

    let mut bad_colour = draft("Day", false);
    bad_colour.tokens.insert("window".into(), "white".into());
    let refused = engine.dispatch(save(bad_colour)).await;
    assert!(matches!(refused, Err(EngineError::ThemeInvalid { .. })));

    let mut unknown = draft("Day", false);
    unknown.tokens.insert("sparkle".into(), "#ffffff".into());
    let refused = engine.dispatch(save(unknown)).await;
    assert!(matches!(refused, Err(EngineError::ThemeInvalid { .. })));

    let blank = engine.dispatch(save(draft(" \n ", false))).await;
    assert!(matches!(blank, Err(EngineError::ThemeInvalid { .. })));

    let mut over_builtin = draft("Mine", false);
    over_builtin.id = Some("fluent-light".into());
    let refused = engine.dispatch(save(over_builtin)).await;
    assert!(matches!(refused, Err(EngineError::ThemeBuiltin { .. })));
    let refused = engine
        .dispatch(Command::DeleteTheme {
            id: "nord-dark".into(),
        })
        .await;
    assert!(matches!(refused, Err(EngineError::ThemeBuiltin { .. })));
}

/// A setting that names a theme no longer there falls back to Fluent.
#[tokio::test]
async fn an_unknown_chosen_theme_falls_back_to_fluent() {
    let (_root, engine) = test_support::engine_with_db(&[]).await;
    let db = engine.db().expect("db");
    db.set("appearance.theme_light", &"own-gone")
        .await
        .expect("set");
    let state = crate::settings::themes::load(db).await;
    assert_eq!(state.chosen[0], "fluent-light");
}
