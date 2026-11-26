use rc_engine::settings::themes::BUILTIN_THEMES;

use super::{follows_system_accent, look, name};
use crate::theme::{
    colours::{MIN_CONTRAST, contrast, low_contrast, over},
    status::{MIN_BORDER_CONTRAST, backgrounds},
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

/// RC-110-77: every pair the audit measured, from every built-in look's tokens - danger, warning
/// and success at 4.5:1 on the window, the surface, the header and the lists; the fields' border
/// at 3:1 on each of them; the inactive panel's cursor outline at 3:1 on the list and on the
/// cursor's own fill.
#[test]
fn every_builtin_reads_its_status_its_fields_and_its_inactive_cursor() {
    for theme in BUILTIN_THEMES.iter().filter(|theme| theme.available()) {
        let accent = if theme.dark {
            DEFAULT_ACCENT.dark
        } else {
            DEFAULT_ACCENT.light
        };
        let tokens = look(theme.id, accent).expect("a look");
        let status = tokens.status;
        for back in backgrounds(&tokens) {
            for (what, colour) in [
                ("danger", status.danger),
                ("warning", status.warning),
                ("success", status.success),
            ] {
                let ratio = contrast(colour, back);
                assert!(ratio >= MIN_CONTRAST, "{} {what}: {ratio:.2}:1", theme.id);
            }
            let ratio = contrast(status.field, back);
            assert!(
                ratio >= MIN_BORDER_CONTRAST,
                "{} field border: {ratio:.2}:1",
                theme.id
            );
        }
        let list = tokens.list_fill;
        for back in [list, over(tokens.cursor_fill_inactive, list)] {
            let ratio = contrast(tokens.list_text_secondary, back);
            assert!(
                ratio >= MIN_BORDER_CONTRAST,
                "{} inactive cursor outline: {ratio:.2}:1",
                theme.id
            );
        }
    }
}

/// The palette themes bring their own terminal colours, which differ from Fluent's, in both
/// modes.
#[test]
fn the_palette_themes_have_their_terminals() {
    let fluent_light = look("fluent-light", DEFAULT_ACCENT.light).expect("fluent");
    let fluent_dark = look("fluent-dark", DEFAULT_ACCENT.dark).expect("fluent");
    for family in [
        "solarized",
        "nord",
        "gruvbox",
        "dracula",
        "catppuccin",
        "tokyo-night",
        "everforest",
        "rose-pine",
        "kanagawa",
        "ayu",
    ] {
        let light = look(&format!("{family}-light"), DEFAULT_ACCENT.light).expect("a look");
        let dark = look(&format!("{family}-dark"), DEFAULT_ACCENT.dark).expect("a look");
        assert!(!light.dark && dark.dark, "{family}");
        assert_ne!(light.terminal, fluent_light.terminal, "{family}");
        assert_ne!(dark.terminal, fluent_dark.terminal, "{family}");
        assert!(
            !follows_system_accent(&format!("{family}-dark")),
            "{family}"
        );
    }
    let solarized = look("solarized-dark", DEFAULT_ACCENT.dark).expect("solarized");
    assert_eq!(
        solarized.terminal.background,
        iced::Color::from_rgb8(0x00, 0x2b, 0x36)
    );
    let dracula = look("dracula-dark", DEFAULT_ACCENT.dark).expect("dracula");
    assert_eq!(
        dracula.terminal.background,
        iced::Color::from_rgb8(0x28, 0x2a, 0x36)
    );
}

/// RC-110-78: every built-in look draws a text field's placeholder at 4.5:1 on the field, where
/// iced's own grey fell to 2.95:1; a disabled field keeps iced's look.
#[test]
fn every_builtin_draws_a_readable_placeholder() {
    use iced::{Background, widget::text_input::Status};

    for theme in BUILTIN_THEMES.iter().filter(|theme| theme.available()) {
        let accent = if theme.dark {
            DEFAULT_ACCENT.dark
        } else {
            DEFAULT_ACCENT.light
        };
        let tokens = look(theme.id, accent).expect("a look");
        let drawn = crate::theme::iced_theme(&tokens);
        for state in [
            Status::Active,
            Status::Hovered,
            Status::Focused { is_hovered: false },
        ] {
            let style = crate::theme::text_field(&drawn, state);
            let Background::Color(field) = style.background else {
                panic!("{}: a field of one colour", theme.id);
            };
            let ratio = contrast(style.placeholder, field);
            assert!(
                ratio >= MIN_CONTRAST,
                "{} placeholder: {ratio:.2}:1",
                theme.id
            );
        }
        let disabled = crate::theme::text_field(&drawn, Status::Disabled);
        assert_eq!(
            disabled,
            iced::widget::text_input::default(&drawn, Status::Disabled)
        );
    }
}
