use iced::Color;

use super::{Access, SystemColours, apply, contrast_look, scale_of, text_scale};
use crate::theme::{
    builtin::system_high_contrast,
    colours::{MIN_CONTRAST, contrast, low_contrast},
    hex,
};

/// Windows 11's contrast themes, as Settings, Accessibility, Contrast themes shows them.
const AQUATIC: SystemColours = SystemColours {
    window: 0x202020,
    window_text: 0xFFFFFF,
    highlight: 0x8EE3F0,
    highlight_text: 0x263B50,
    hot_light: 0x75E9FC,
    gray_text: 0xA6A6A6,
};
const DESERT: SystemColours = SystemColours {
    window: 0xFFFAEF,
    window_text: 0x3D3D3D,
    highlight: 0x903909,
    highlight_text: 0xFFF5E3,
    hot_light: 0x1C5E75,
    gray_text: 0x676767,
};
const DUSK: SystemColours = SystemColours {
    window: 0x2D3236,
    window_text: 0xB6F6F0,
    highlight: 0xA6E7EF,
    highlight_text: 0x212D3B,
    hot_light: 0x70EBDE,
    gray_text: 0xA6A6A6,
};
const NIGHT_SKY: SystemColours = SystemColours {
    window: 0x000000,
    window_text: 0xFFFFFF,
    highlight: 0xD6B4FD,
    highlight_text: 0x2B2B2B,
    hot_light: 0x8080FF,
    gray_text: 0xA6A6A6,
};

fn colour(rgb: u32) -> Color {
    hex(rgb, 1.0)
}

/// Windows' text size of 100 to 225 % is the scale; anything else, or none, draws at 1.
#[test]
fn the_text_size_is_the_scale_within_windows_range() {
    assert!((scale_of(None) - 1.0).abs() < f32::EPSILON);
    assert!((scale_of(Some(100)) - 1.0).abs() < f32::EPSILON);
    assert!((scale_of(Some(150)) - 1.5).abs() < f32::EPSILON);
    assert!((scale_of(Some(225)) - 2.25).abs() < f32::EPSILON);
    assert!((scale_of(Some(99)) - 1.0).abs() < f32::EPSILON);
    assert!((scale_of(Some(226)) - 1.0).abs() < f32::EPSILON);
    assert!((scale_of(Some(u32::MAX)) - 1.0).abs() < f32::EPSILON);
}

/// The look of each of Windows' contrast themes draws in its colours - every background the
/// window colour, the text its text colour, the cursor its highlight - in the mode of its window,
/// and passes the Themes tab's check.
#[test]
fn a_contrast_theme_draws_in_the_systems_colours() {
    for (name, colours, dark) in [
        ("aquatic", AQUATIC, true),
        ("desert", DESERT, false),
        ("dusk", DUSK, true),
        ("night sky", NIGHT_SKY, true),
    ] {
        let tokens = system_high_contrast(&colours);
        assert_eq!(tokens.dark, dark, "{name}");
        let window = colour(colours.window);
        let text = colour(colours.window_text);
        for background in [
            tokens.window,
            tokens.surface,
            tokens.header,
            tokens.list_fill,
            tokens.control,
            tokens.terminal.background,
        ] {
            assert_eq!(background, window, "{name}");
        }
        for foreground in [
            tokens.text,
            tokens.list_text,
            tokens.stroke,
            tokens.terminal.foreground,
        ] {
            assert_eq!(foreground, text, "{name}");
        }
        assert_eq!(tokens.cursor_fill, colour(colours.highlight), "{name}");
        assert_eq!(tokens.cursor_text, colour(colours.highlight_text), "{name}");
        assert_eq!(tokens.marked, colour(colours.hot_light), "{name}");
        assert!(low_contrast(&tokens).is_empty(), "{name}");
        assert!(
            contrast(tokens.cursor_text, tokens.cursor_fill_inactive) >= MIN_CONTRAST,
            "{name}: the inactive cursor"
        );
        let syntax = tokens.syntax;
        for class in [
            syntax.keyword,
            syntax.key,
            syntax.string,
            syntax.number,
            syntax.literal,
            syntax.comment,
            syntax.punctuation,
            syntax.operator,
            syntax.type_name,
            syntax.function,
            syntax.invalid,
        ] {
            assert!(contrast(class, window) >= MIN_CONTRAST, "{name}: syntax");
        }
    }
}

/// A link colour, or a grey for the inactive cursor, that would not read is moved until it does.
#[test]
fn weak_system_colours_are_made_readable() {
    let weak = SystemColours {
        hot_light: AQUATIC.window,
        gray_text: AQUATIC.highlight_text,
        ..AQUATIC
    };
    let tokens = system_high_contrast(&weak);
    assert!(contrast(tokens.accent, tokens.window) >= MIN_CONTRAST);
    assert!(contrast(tokens.marked, tokens.list_fill) >= MIN_CONTRAST);
    assert!(contrast(tokens.cursor_text, tokens.cursor_fill_inactive) >= MIN_CONTRAST);
}

/// While high contrast is on every look is its look; the scale is the text size's; each change is
/// told once.
#[test]
fn applied_settings_take_over_every_look() {
    let on = Access {
        contrast: Some(DESERT),
        text_scale: 1.5,
    };
    assert!(apply(on));
    assert!(!apply(on), "nothing changed");
    let look = contrast_look().expect("high contrast is on");
    assert!(std::ptr::eq(crate::theme::tokens(false), look));
    assert!(std::ptr::eq(crate::theme::tokens(true), look));
    assert!((text_scale() - 1.5).abs() < f32::EPSILON);
    let off = Access {
        contrast: None,
        text_scale: 1.0,
    };
    assert!(apply(off));
    assert!(contrast_look().is_none());
    assert!((text_scale() - 1.0).abs() < f32::EPSILON);
    assert!(!std::ptr::eq(crate::theme::tokens(false), look));
}

/// Outside Windows nothing is read: no high contrast, a scale of 1, no screen reader.
#[cfg(not(windows))]
#[test]
fn elsewhere_the_system_asks_nothing() {
    let read = super::read();
    assert!(read.contrast.is_none());
    assert!((read.text_scale - 1.0).abs() < f32::EPSILON);
    assert!(!super::screen_reader());
}
