use std::collections::BTreeMap;

use rc_engine::settings::themes::{BUILTIN_THEMES, Theme, ThemeAccent, ThemesState};

use super::{ThemesPage, draft_of, free_name, shown_name};

fn own(name: &str) -> Theme {
    Theme {
        id: "own-1".into(),
        name: name.into(),
        dark: true,
        accent: ThemeAccent::Colour("#ff8000".into()),
        tokens: BTreeMap::from([("window".to_owned(), "#101010".to_owned())]),
        builtin: false,
    }
}

#[test]
fn a_draft_keeps_the_theme_and_a_new_one_has_no_id() {
    let theme = own("Night");
    let kept = draft_of(&theme, false);
    assert_eq!(kept.id.as_deref(), Some("own-1"));
    assert_eq!(kept.accent, "#ff8000");
    assert_eq!(kept.tokens, theme.tokens);
    assert!(draft_of(&theme, true).id.is_none());
}

/// A built-in theme is shown under its name in the interface's language, an own one under its
/// own; a new name never repeats an own theme's.
#[test]
fn names_are_shown_and_kept_apart() {
    let nord = BUILTIN_THEMES
        .iter()
        .find(|theme| theme.id == "nord-dark")
        .map(Theme::of_builtin)
        .expect("nord");
    assert!(!shown_name(&nord).is_empty());
    assert_eq!(shown_name(&own("Night")), "Night");
    let page = ThemesPage {
        state: Some(ThemesState {
            themes: vec![nord, own("Night")],
            chosen: ["fluent-light".into(), "own-1".into()],
        }),
        ..ThemesPage::default()
    };
    assert_eq!(free_name(&page, "Night"), "Night (2)");
    assert_eq!(free_name(&page, "Nord"), "Nord", "a built-in name is free");
    assert!(page.chosen("own-1"));
}
