use iced::Color;

use super::{
    ThemeChoice, dark, light,
    system::{DEFAULT_ACCENT, from_palette},
    thumb_fill, tokens,
};

#[test]
fn the_choice_decides_light_or_dark() {
    for system_dark in [false, true] {
        assert_eq!(ThemeChoice::System.is_dark(system_dark), system_dark);
        assert!(!ThemeChoice::Light.is_dark(system_dark));
        assert!(ThemeChoice::Dark.is_dark(system_dark));
    }
}

#[test]
fn the_setting_names_the_choice_and_anything_else_is_system() {
    assert_eq!(ThemeChoice::from_setting("light"), ThemeChoice::Light);
    assert_eq!(ThemeChoice::from_setting("dark"), ThemeChoice::Dark);
    assert_eq!(ThemeChoice::from_setting("system"), ThemeChoice::System);
    assert_eq!(ThemeChoice::from_setting("neon"), ThemeChoice::System);
}

#[test]
fn both_looks_are_complete_and_tell_themselves_apart() {
    assert!(!tokens(false).dark);
    assert!(tokens(true).dark);
    assert_ne!(tokens(false).window, tokens(true).window);
    assert_ne!(tokens(false).text, tokens(true).text);
}

#[test]
fn the_accent_tints_cursor_and_selection() {
    let accent = Color::from_rgb8(0x12, 0x34, 0x56);
    for look in [light::tokens(accent), dark::tokens(accent)] {
        assert_eq!(look.accent, accent);
        assert_eq!(
            Color {
                a: 1.0,
                ..look.cursor_fill
            },
            accent
        );
        assert_eq!(
            Color {
                a: 1.0,
                ..look.selected_fill
            },
            accent
        );
    }
}

#[test]
fn the_palette_gives_dark1_for_light_and_light2_for_dark() {
    let mut palette = [0_u8; 32];
    palette[4..8].copy_from_slice(&[0x60, 0xCD, 0xFF, 0x00]);
    palette[16..20].copy_from_slice(&[0x00, 0x5F, 0xB8, 0x00]);
    let accent = from_palette(&palette).expect("a palette");
    assert_eq!(accent, DEFAULT_ACCENT);
}

#[test]
fn a_malformed_palette_is_no_accent() {
    assert_eq!(from_palette(&[0; 31]), None);
    assert_eq!(from_palette(&[]), None);
}

/// RC-010-23: the scroll bar's thumb is stronger under the pointer and strongest while held.
#[test]
fn the_thumb_grows_stronger_hovered_and_pressed() {
    for dark in [false, true] {
        let look = tokens(dark);
        let idle = thumb_fill(look, false, false).a;
        let hovered = thumb_fill(look, true, false).a;
        let pressed = thumb_fill(look, true, true).a;
        assert!(
            idle < hovered && hovered < pressed,
            "{idle} {hovered} {pressed}"
        );
        assert_eq!(
            thumb_fill(look, false, true).a,
            pressed,
            "held off the thumb"
        );
    }
}

/// WCAG's relative luminance of a colour.
fn luminance(color: Color) -> f32 {
    let linear = |c: f32| {
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// WCAG's contrast ratio of two colours, 1 to 21.
fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// RC-100-10: every token class of the coloured text reads at 4.5:1 or more on the viewer's
/// surface in both looks (WCAG AA for text), as the text itself does.
#[test]
fn the_syntax_colours_read_on_the_surface_in_both_looks() {
    for dark in [false, true] {
        let look = tokens(dark);
        let s = look.syntax;
        let classes = [
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
        ];
        assert!(contrast(look.text, look.surface) >= 4.5, "the text itself");
        for (name, color) in classes {
            let ratio = contrast(color, look.surface);
            assert!(ratio >= 4.5, "syntax.{name} in dark={dark}: {ratio:.2}:1");
        }
        assert_ne!(s.string, s.number, "the classes tell themselves apart");
    }
    assert_ne!(tokens(false).syntax, tokens(true).syntax);
}
