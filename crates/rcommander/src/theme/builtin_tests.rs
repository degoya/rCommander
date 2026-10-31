use rc_engine::settings::themes::BUILTIN_THEMES;

use super::{follows_system_accent, look, name};
use crate::theme::{
    colours::{MIN_CONTRAST, contrast, low_contrast},
    system::DEFAULT_ACCENT,
};

/// The engine's catalogue and the window's looks are one list: every built-in theme this system
/// offers has its look, its name and its mode, and none more.
#[test]
fn every_builtin_of_the_engine_has_its_look() {
    for theme in BUILTIN_THEMES.iter().filter(|theme| theme.available()) {
        let accent = if theme.dark {
            DEFAULT_ACCENT.dark
        } else {
            DEFAULT_ACCENT.light
        };
        let tokens = look(theme.id, accent).unwrap_or_else(|| panic!("{} has no look", theme.id));
        assert_eq!(tokens.dark, theme.dark, "{}", theme.id);
        assert!(name(theme.id).is_some(), "{}", theme.id);
    }
    assert!(look("own-1", DEFAULT_ACCENT.light).is_none());
    assert!(look("neon", DEFAULT_ACCENT.light).is_none());
    assert!(follows_system_accent("fluent-dark"));
    assert!(!follows_system_accent("nord-dark"));
}

/// Every built-in look passes the Themes tab's check - the lists, the window's text, the terminal
/// - and its syntax colours read at 4.5:1 on its surface (WCAG AA for text).
#[test]
fn every_builtin_passes_the_contrast_check() {
    for theme in BUILTIN_THEMES.iter().filter(|theme| theme.available()) {
        let accent = if theme.dark {
            DEFAULT_ACCENT.dark
        } else {
            DEFAULT_ACCENT.light
        };
        let tokens = look(theme.id, accent).expect("a look");
        let low = low_contrast(&tokens);
        assert!(low.is_empty(), "{}: {low:?}", theme.id);
        let s = tokens.syntax;
        for (class, colour) in [
            ("keyword", s.keyword),
            ("key", s.key),
            ("string", s.string),
            ("number", s.number),
            ("literal", s.literal),
            ("comment", s.comment),
            ("punctuation", s.punctuation),
            ("operator", s.operator),
            ("type_name", s.type_name),
            ("function", s.function),
            ("invalid", s.invalid),
        ] {
            let ratio = contrast(colour, tokens.surface);
            assert!(
                ratio >= MIN_CONTRAST,
                "{} syntax.{class}: {ratio:.2}:1",
                theme.id
            );
        }
        assert_ne!(s.string, s.number, "{}", theme.id);
    }
}

/// Solarized and Nord bring their own terminal colours, which differ from Fluent's.
#[test]
fn solarized_and_nord_have_their_terminals() {
    let fluent = look("fluent-dark", DEFAULT_ACCENT.dark).expect("fluent");
    for id in ["solarized-dark", "nord-dark"] {
        let tokens = look(id, DEFAULT_ACCENT.dark).expect("a look");
        assert_ne!(tokens.terminal, fluent.terminal, "{id}");
    }
    let solarized = look("solarized-dark", DEFAULT_ACCENT.dark).expect("solarized");
    assert_eq!(
        solarized.terminal.background,
        iced::Color::from_rgb8(0x00, 0x2b, 0x36)
    );
}
