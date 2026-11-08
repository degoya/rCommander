use rc_engine::{
    SETTINGS, SettingValue,
    settings::{
        THEME_DARK_KEY, THEME_LIGHT_KEY, WSL_THEMES_KEY,
        themes::{BUILTIN_THEMES, Theme, ThemesState},
    },
};
use rc_mcp::Policy;

use super::{Environment, assign, mode_theme, offered, used_by, uses};
use crate::app::settings::draft::{Draft, Field, Loaded};

/// Every built-in theme this system offers, Fluent chosen for both modes.
fn builtins() -> ThemesState {
    ThemesState {
        themes: BUILTIN_THEMES
            .iter()
            .filter(|theme| theme.available())
            .map(Theme::of_builtin)
            .collect(),
        chosen: ["fluent-light".into(), "fluent-dark".into()],
    }
}

/// A draft of every setting at its default.
fn draft() -> Draft {
    let values = SETTINGS
        .iter()
        .map(|spec| SettingValue {
            key: spec.key.to_owned(),
            value: spec.default_value(),
            default: spec.default_value(),
            kind: String::new(),
            min: None,
            max: None,
            choices: Vec::new(),
            protected: false,
            restart: false,
        })
        .collect();
    Draft::new(Loaded {
        values,
        retention: Vec::new(),
        policy: Policy::default(),
        shells: Vec::new(),
        fonts: Vec::new(),
    })
}

fn theme<'a>(state: &'a ThemesState, id: &str) -> &'a Theme {
    state.theme(id).expect("a built-in theme")
}

fn distro(name: &str, dark: bool) -> Environment {
    Environment::Distro {
        name: name.to_owned(),
        dark,
    }
}

#[test]
fn a_theme_is_offered_for_its_own_mode_and_each_distribution_in_it() {
    let state = builtins();
    let distros = vec!["Ubuntu".to_owned(), "Debian".to_owned()];
    assert_eq!(
        offered(theme(&state, "nord-dark"), &distros),
        vec![
            Environment::Mode { dark: true },
            distro("Ubuntu", true),
            distro("Debian", true),
        ]
    );
    assert_eq!(
        offered(theme(&state, "nord-light"), &[]),
        vec![Environment::Mode { dark: false }],
        "without the WSL section, the mode alone"
    );
}

#[test]
fn the_draft_decides_which_theme_a_mode_uses() {
    let state = builtins();
    let mut draft = draft();
    draft.edit(THEME_DARK_KEY, Field::Text("gone".into()));
    assert_eq!(
        mode_theme(&draft, &state, true),
        "fluent-dark",
        "an unknown id falls back to the chosen one"
    );
    draft.edit(THEME_DARK_KEY, Field::Text("nord-dark".into()));
    assert_eq!(mode_theme(&draft, &state, true), "nord-dark");
    let dark = Environment::Mode { dark: true };
    assert!(uses(&dark, "nord-dark", &draft, &state));
    assert!(!uses(&dark, "fluent-dark", &draft, &state));
}

#[test]
fn using_a_theme_edits_what_the_pick_list_of_the_environment_edits() {
    let state = builtins();
    let mut draft = draft();
    let (key, text) = assign(
        &Environment::Mode { dark: false },
        "nord-light",
        &draft,
        &state,
    );
    assert_eq!((key, text.as_str()), (THEME_LIGHT_KEY, "nord-light"));
    let (key, text) = assign(
        &Environment::Mode { dark: true },
        "nord-dark",
        &draft,
        &state,
    );
    assert_eq!((key, text.as_str()), (THEME_DARK_KEY, "nord-dark"));
    draft.edit(WSL_THEMES_KEY, Field::Text("Ubuntu\tnord-light\t".into()));
    let (key, text) = assign(&distro("UBUNTU", true), "nord-dark", &draft, &state);
    assert_eq!(key, WSL_THEMES_KEY);
    assert_eq!(text, "Ubuntu\tnord-light\tnord-dark", "the light one stays");
    let (_, text) = assign(&distro("Debian", false), "classic-light", &draft, &state);
    assert_eq!(text, "Ubuntu\tnord-light\t\nDebian\tclassic-light\t");
}

#[test]
fn a_theme_lists_the_environments_that_use_it() {
    let state = builtins();
    let mut draft = draft();
    draft.edit(THEME_DARK_KEY, Field::Text("nord-dark".into()));
    draft.edit(
        WSL_THEMES_KEY,
        Field::Text(
            "Ubuntu\t\tnord-dark\nKali\tnord-light\tnord-dark\nArch\t\tsolarized-dark".into(),
        ),
    );
    let nord = theme(&state, "nord-dark");
    assert_eq!(
        used_by(nord, &draft, &state, true),
        vec![
            Environment::Mode { dark: true },
            distro("Ubuntu", true),
            distro("Kali", true),
        ]
    );
    assert_eq!(
        used_by(nord, &draft, &state, false),
        vec![Environment::Mode { dark: true }],
        "no WSL section, no distributions"
    );
    assert!(uses(&distro("kali", true), "nord-dark", &draft, &state));
    assert!(!uses(&distro("Arch", true), "nord-dark", &draft, &state));
    assert!(
        used_by(theme(&state, "fluent-dark"), &draft, &state, true).is_empty(),
        "the draft moved the dark mode away from it"
    );
}

#[test]
fn every_environment_has_a_tag_and_an_entry() {
    for environment in [
        Environment::Mode { dark: false },
        Environment::Mode { dark: true },
        distro("Ubuntu", false),
    ] {
        assert!(!environment.tag().is_empty(), "{environment:?}");
        assert!(!environment.use_label().is_empty(), "{environment:?}");
    }
    assert!(distro("Ubuntu", true).tag().contains("Ubuntu"));
    assert!(distro("Ubuntu", true).use_label().contains("Ubuntu"));
}
