use std::collections::BTreeMap;

use iced::Color;
use rc_engine::settings::themes::{BUILTIN_THEMES, Theme, ThemeAccent, ThemesState};

use super::{build, current, end_preview, look, preview_owner, set_preview, set_state, tokens_of};
use crate::theme::{dark, light, system::DEFAULT_ACCENT, terminal_palette};

fn own(dark: bool, tokens: &[(&str, &str)], accent: ThemeAccent) -> Theme {
    Theme {
        id: "own-test".into(),
        name: "Test".into(),
        dark,
        accent,
        tokens: tokens
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>(),
        builtin: false,
    }
}

/// A theme that sets every token draws with those; one that sets a few keeps the fixed set's for
/// the rest.
#[test]
fn a_theme_makes_its_look_complete_or_gappy() {
    let gappy = own(
        true,
        &[("window", "#101010"), ("terminal.red", "#ff0000")],
        ThemeAccent::System,
    );
    let tokens = build(&gappy, DEFAULT_ACCENT.dark);
    let fixed = dark::tokens(DEFAULT_ACCENT.dark);
    assert_eq!(tokens.window, Color::from_rgb8(0x10, 0x10, 0x10));
    assert_eq!(tokens.terminal.red, Color::from_rgb8(0xff, 0, 0));
    assert_eq!(tokens.surface, fixed.surface, "the fixed set's");
    assert_eq!(tokens.terminal.background, fixed.terminal.background);
    assert!(tokens.dark);

    let all: Vec<(&str, &str)> = super::KEYS.iter().map(|key| (*key, "#123456")).collect();
    let complete = build(&own(false, &all, ThemeAccent::System), DEFAULT_ACCENT.light);
    for key in super::KEYS {
        assert_eq!(
            super::slots::value(&complete, key),
            Some(Color::from_rgb8(0x12, 0x34, 0x56)),
            "{key}"
        );
    }
}

/// An own accent replaces the system's, and the cursor's tint is made of it.
#[test]
fn an_own_accent_tints_the_cursor() {
    let orange = Color::from_rgb8(0xff, 0x80, 0x00);
    let theme = own(false, &[], ThemeAccent::Colour("#ff8000".into()));
    let tokens = build(&theme, DEFAULT_ACCENT.light);
    assert_eq!(tokens.accent, orange);
    assert_eq!(tokens.marked, orange);
    assert_eq!(tokens.cursor_fill, light::tokens(orange).cursor_fill);
}

/// The terminal's palette is the theme's.
#[test]
fn the_terminal_takes_the_themes_colours() {
    let theme = own(
        true,
        &[
            ("terminal.foreground", "#eeeeee"),
            ("terminal.background", "#000000"),
        ],
        ThemeAccent::System,
    );
    let palette = terminal_palette(&build(&theme, DEFAULT_ACCENT.dark));
    assert_eq!(palette.foreground, "#eeeeee");
    assert_eq!(palette.background, "#000000");
    assert_eq!(palette.dim_foreground, palette.bright_black);
}

/// A theme seen again takes the look made for it; another accent or another colour makes a new
/// one. A built-in theme whose look this system lacks draws with the fixed set of its mode.
#[test]
fn a_look_is_made_once_per_theme() {
    let theme = own(true, &[("header", "#202020")], ThemeAccent::System);
    let first = look(&theme);
    assert!(std::ptr::eq(first, look(&theme.clone())));
    let other = own(true, &[("header", "#212121")], ThemeAccent::System);
    assert!(!std::ptr::eq(first, look(&other)));
    let foreign = Theme {
        builtin: true,
        ..own(false, &[], ThemeAccent::System)
    };
    assert_eq!(
        build(&foreign, DEFAULT_ACCENT.light).surface,
        light::tokens(DEFAULT_ACCENT.light).surface
    );
}

/// The chosen looks and the editor's preview, with themes that draw as Fluent does, so the other
/// tests of the run see Fluent's colours whatever runs at the same time.
#[test]
fn the_preview_draws_its_mode_until_it_is_taken_away() {
    let themes: Vec<Theme> = BUILTIN_THEMES
        .iter()
        .filter(|theme| theme.id.starts_with("fluent-"))
        .map(Theme::of_builtin)
        .collect();
    let state = ThemesState {
        themes,
        chosen: ["fluent-light".into(), "fluent-dark".into()],
    };
    set_state(&state);
    let chosen = current(true).expect("the chosen dark look");
    assert!(std::ptr::eq(
        chosen,
        tokens_of("fluent-dark", true).expect("known")
    ));
    assert!(tokens_of("own-unknown", true).is_none());
    assert!(
        tokens_of("fluent-dark", false).is_none(),
        "a dark theme is no look of the light mode"
    );

    let edited = own(true, &[], ThemeAccent::System);
    let owner = preview_owner();
    set_preview(owner, &edited);
    assert!(std::ptr::eq(current(true).expect("preview"), look(&edited)));
    assert!(!std::ptr::eq(current(false).expect("light"), look(&edited)));
    end_preview(owner + 1_000_000);
    assert!(
        std::ptr::eq(current(true).expect("still"), look(&edited)),
        "another editor's end"
    );
    end_preview(owner);
    assert!(std::ptr::eq(current(true).expect("chosen again"), chosen));
}
