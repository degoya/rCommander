use iced::Color;

use super::{
    Colours, MIN_CONTRAST, build, chosen, contrast, low_contrast, over, parse_hex, set_colours,
};

const ACCENT: Color = Color::from_rgb(0.0, 0.37, 0.72);

/// The accent of each look as the fixed looks draw with it by default.
fn accent(dark: bool) -> Color {
    let default = super::super::system::DEFAULT_ACCENT;
    if dark { default.dark } else { default.light }
}

#[test]
fn hex_reads_as_a_colour() {
    assert_eq!(
        parse_hex("#FF8000"),
        Some(Color::from_rgb8(0xff, 0x80, 0x00))
    );
    assert_eq!(parse_hex("#0f0"), Some(Color::from_rgb8(0, 0xff, 0)));
    assert_eq!(parse_hex(""), None);
    assert_eq!(parse_hex("green"), None);
}

/// A chosen colour replaces its token only; the window's own text and surface stay, so a dialog
/// keeps its colours whatever the lists look like.
#[test]
fn a_chosen_colour_replaces_its_token_only() {
    let plain = build(false, ACCENT, &Colours::default());
    let black = Color::from_rgb8(0, 0, 0);
    let white = Color::from_rgb8(0xff, 0xff, 0xff);
    let chosen = build(
        false,
        ACCENT,
        &Colours {
            background: Some(black),
            text: Some(white),
            ..Colours::default()
        },
    );
    assert_eq!(chosen.list_fill, black);
    assert_eq!(chosen.list_text, white);
    // The cursor's text follows the lists' text unless it is chosen itself.
    assert_eq!(chosen.cursor_text, white);
    assert_eq!(chosen.surface, plain.surface);
    assert_eq!(chosen.text, plain.text);
    assert_eq!(chosen.cursor_fill, plain.cursor_fill);
    assert_eq!(chosen.marked, plain.marked);
}

/// The accent chosen replaces the system's, and what is made of it - the cursor's tint - with it.
#[test]
fn a_chosen_accent_tints_the_cursor() {
    let orange = Color::from_rgb8(0xff, 0x80, 0x00);
    let dark = build(
        true,
        ACCENT,
        &Colours {
            accent: Some(orange),
            ..Colours::default()
        },
    );
    assert_eq!(dark.accent, orange);
    assert_eq!(dark.marked, orange);
    assert_eq!(dark.cursor_fill, Color { a: 0.16, ..orange });
}

#[test]
fn contrast_follows_wcag() {
    let black = Color::BLACK;
    let white = Color::WHITE;
    assert!((contrast(black, white) - 21.0).abs() < 0.01);
    assert!((contrast(white, white) - 1.0).abs() < 0.01);
    let grey = Color::from_rgb8(0x77, 0x77, 0x77);
    assert!(contrast(grey, white) < MIN_CONTRAST);
    // A translucent fill is judged as it is seen over the opaque one.
    let seen = over(Color { a: 0.5, ..black }, white);
    assert!((seen.r - 0.5).abs() < 0.01);
}

/// The fixed looks pass; grey text on white and yellow on white do not.
#[test]
fn low_contrast_names_the_pairs_below_aa() {
    for dark in [false, true] {
        let plain = build(dark, accent(dark), &Colours::default());
        assert!(
            low_contrast(&plain).is_empty(),
            "dark {dark}: {:?}",
            low_contrast(&plain)
        );
    }
    let washed = build(
        false,
        ACCENT,
        &Colours {
            text: Some(Color::from_rgb8(0xbb, 0xbb, 0xbb)),
            marked: Some(Color::from_rgb8(0xff, 0xff, 0x00)),
            ..Colours::default()
        },
    );
    let low = low_contrast(&washed);
    assert!(low.contains(&"settings.colours.low_text"), "{low:?}");
    assert!(low.contains(&"settings.colours.low_marked"), "{low:?}");
}

/// RC-080-53: a look without a chosen colour is the fixed one, and colours chosen again take the
/// look made for them before rather than a new one.
#[test]
fn a_look_is_made_once_per_set_of_colours() {
    let dark_only = [
        Colours::default(),
        Colours {
            background: Some(Color::BLACK),
            ..Colours::default()
        },
    ];
    set_colours(dark_only, [ACCENT; 2]);
    assert!(
        chosen(false).is_none(),
        "the light look stays the fixed one"
    );
    let first = chosen(true).expect("the dark look");
    set_colours([Colours::default(); 2], [ACCENT; 2]);
    assert!(chosen(true).is_none());
    set_colours(dark_only, [ACCENT; 2]);
    assert!(std::ptr::eq(first, chosen(true).expect("chosen again")));
}
