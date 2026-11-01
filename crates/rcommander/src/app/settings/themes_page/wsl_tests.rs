use rc_engine::settings::{
    parse_wsl_themes,
    themes::{BUILTIN_THEMES, Theme, ThemesState},
};

use super::{known_rows, with_theme};

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

#[test]
fn a_theme_the_window_does_not_offer_for_its_mode_shows_as_default() {
    let rows = known_rows(
        "Ubuntu\tnord-light\town-gone\nDebian\tnord-dark\tsolarized-dark",
        &builtins(),
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].light.as_deref(), Some("nord-light"));
    assert_eq!(rows[0].dark, None, "deleted");
    assert_eq!(rows[1].light, None, "of the other mode");
    assert_eq!(rows[1].dark.as_deref(), Some("solarized-dark"));
    assert!(known_rows("not a list", &builtins()).is_empty());
}

#[test]
fn a_pick_sets_its_distribution_s_mode_and_keeps_the_rest() {
    let rows = known_rows("Ubuntu\tnord-light\t", &builtins());
    let text = with_theme(rows.clone(), "UBUNTU", true, Some("nord-dark".into()));
    assert_eq!(text, "Ubuntu\tnord-light\tnord-dark", "case-insensitive");
    let text = with_theme(rows.clone(), "Debian", false, Some("classic-light".into()));
    assert_eq!(text, "Ubuntu\tnord-light\t\nDebian\tclassic-light\t");
    let text = with_theme(rows, "Ubuntu", false, None);
    assert_eq!(text, "", "Default for both modes leaves no line");
    let read = parse_wsl_themes(&with_theme(
        Vec::new(),
        "Kali",
        true,
        Some("nord-dark".into()),
    ));
    assert_eq!(
        read.map(|rows| rows.len()),
        Some(1),
        "what a pick writes reads"
    );
}
