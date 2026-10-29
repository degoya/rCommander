use iced_core::text::Shaping;

use super::shaping;

#[test]
fn only_one_ascii_glyph_skips_advanced_shaping() {
    assert_eq!(shaping("a"), Shaping::Basic);
    assert_eq!(shaping("~"), Shaping::Basic);
    assert_eq!(shaping("e\u{301}"), Shaping::Advanced);
    assert_eq!(shaping("\u{fc}"), Shaping::Advanced);
    assert_eq!(shaping("\u{4e2d}"), Shaping::Advanced);
    assert_eq!(shaping("\u{2500}"), Shaping::Advanced);
}
