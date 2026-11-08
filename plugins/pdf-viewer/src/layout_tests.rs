//! Glyph positions into lines and words.

use super::{Layout, MAX_PAGE_BYTES};

/// Each character of `word` from `x` on at a 6-point advance (half of a 12-point size).
fn word(layout: &mut Layout, x: f64, y: f64, word: &str) {
    for (index, c) in word.chars().enumerate() {
        layout.push(x + 6.0 * index as f64, y, 12.0, &c.to_string());
    }
}

#[test]
fn letters_close_together_are_a_word_and_a_wide_gap_a_space() {
    let mut layout = Layout::default();
    word(&mut layout, 72.0, 700.0, "Hello");
    word(&mut layout, 72.0 + 6.0 * 5.0 + 9.0, 700.0, "PDF");
    assert_eq!(layout.finish(), "Hello PDF\n");
}

#[test]
fn a_glyph_lower_down_starts_a_line() {
    let mut layout = Layout::default();
    word(&mut layout, 72.0, 700.0, "one");
    word(&mut layout, 72.0, 686.0, "two");
    assert_eq!(layout.finish(), "one\ntwo\n");
}

#[test]
fn an_explicit_space_glyph_is_not_doubled() {
    let mut layout = Layout::default();
    word(&mut layout, 72.0, 700.0, "a ");
    word(&mut layout, 72.0 + 30.0, 700.0, "b");
    assert_eq!(layout.finish(), "a b\n");
}

#[test]
fn control_characters_and_odd_values_do_not_break_the_text() {
    let mut layout = Layout::default();
    layout.push(f64::NAN, 1.0, 12.0, "x");
    layout.push(1.0, 1.0, f64::INFINITY, "a\u{7}");
    layout.push(2.0, 1.0, 0.0, "");
    assert_eq!(layout.finish(), "a\n");
    assert_eq!(Layout::default().finish(), "");
}

#[test]
fn a_page_stops_at_its_cap() {
    let mut layout = Layout::default();
    let block = "x".repeat(1 << 16);
    for index in 0..100 {
        layout.push(f64::from(index) * 1e6, 0.0, 12.0, &block);
    }
    assert!(layout.finish().len() <= MAX_PAGE_BYTES + 200);
}
