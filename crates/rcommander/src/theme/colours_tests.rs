use iced::Color;

use super::{MIN_CONTRAST, contrast, low_contrast, over, parse_hex};
use crate::theme::{dark, light, system::DEFAULT_ACCENT};

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

/// The fixed looks pass; grey text on white, yellow marks and a washed-out terminal do not.
#[test]
fn low_contrast_names_the_pairs_below_aa() {
    let plain = [
        light::tokens(DEFAULT_ACCENT.light),
        dark::tokens(DEFAULT_ACCENT.dark),
    ];
    for look in &plain {
        assert!(
            low_contrast(look).is_empty(),
            "dark {}: {:?}",
            look.dark,
            low_contrast(look)
        );
    }
    let mut washed = plain[0].clone();
    washed.list_text = Color::from_rgb8(0xbb, 0xbb, 0xbb);
    washed.marked = Color::from_rgb8(0xff, 0xff, 0x00);
    washed.terminal.foreground = Color::from_rgb8(0xee, 0xee, 0xee);
    let low = low_contrast(&washed);
    for pair in [
        "settings.colours.low_text",
        "settings.colours.low_marked",
        "settings.colours.low_terminal",
    ] {
        assert!(low.contains(&pair), "{pair}: {low:?}");
    }
    assert!(!low.contains(&"settings.colours.low_surface"), "{low:?}");
}
