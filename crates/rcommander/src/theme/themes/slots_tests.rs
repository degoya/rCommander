use iced::Color;
use rc_engine::settings::themes::THEME_TOKENS;

use super::{FROM_ACCENT, KEYS, flatten, hex_of, seen, slot, value};
use crate::theme::{light, system::DEFAULT_ACCENT};

/// The window places every token the engine knows, and no other.
#[test]
fn the_keys_are_the_engines() {
    assert_eq!(KEYS, THEME_TOKENS.as_slice());
    let mut tokens = light::tokens(DEFAULT_ACCENT.light);
    for key in KEYS {
        assert!(value(&tokens, key).is_some(), "{key}");
        assert!(slot(&mut tokens, key).is_some(), "{key}");
    }
    assert!(slot(&mut tokens, "sparkle").is_none());
}

#[test]
fn a_slot_sets_its_token() {
    let mut tokens = light::tokens(DEFAULT_ACCENT.light);
    if let Some(place) = slot(&mut tokens, "terminal.bright_red") {
        *place = Color::BLACK;
    }
    assert_eq!(tokens.terminal.bright_red, Color::BLACK);
    assert_eq!(value(&tokens, "terminal.bright_red"), Some(Color::BLACK));
}

/// A translucent fill is written as it is seen over the lists' fill; an own theme that follows
/// the system's accent leaves out what is made of it.
#[test]
fn flattening_writes_what_is_seen() {
    let tokens = light::tokens(DEFAULT_ACCENT.light);
    let stripe = seen(&tokens, "stripe_fill").expect("stripe");
    assert!((stripe.a - 1.0).abs() < f32::EPSILON);
    let all = flatten(&tokens, false);
    assert_eq!(all.len(), KEYS.len());
    assert_eq!(all.get("stripe_fill"), Some(&hex_of(stripe)));
    assert_eq!(all.get("surface").map(String::as_str), Some("#ffffff"));
    let following = flatten(&tokens, true);
    for key in FROM_ACCENT {
        assert!(!following.contains_key(key), "{key}");
    }
    assert_eq!(hex_of(Color::from_rgb8(0x0a, 0xbc, 0xff)), "#0abcff");
}
