use rc_engine::settings::{LIST_FONT_SIZE_KEY, SettingKind, VIEWER_FONT_SIZE_KEY, setting};

use super::{AREAS, preview_size};

#[test]
fn the_preview_takes_the_typed_size_within_its_range() {
    let list = LIST_FONT_SIZE_KEY;
    assert_eq!(preview_size("16", list), 16.0);
    assert_eq!(preview_size(" 9 ", list), 9.0);
    assert_eq!(preview_size("3", list), 8.0);
    assert_eq!(preview_size("40", list), 24.0);
    assert_eq!(preview_size("big", list), 13.0);
    // The viewer's size reaches 6 to 72 (RC-020-12).
    assert_eq!(preview_size("40", VIEWER_FONT_SIZE_KEY), 40.0);
    assert_eq!(preview_size("3", VIEWER_FONT_SIZE_KEY), 6.0);
}

#[test]
fn every_area_pairs_a_font_row_with_its_size_row() {
    for (font_key, size_key, _) in AREAS {
        let font = setting(font_key).expect("a font row");
        assert!(matches!(font.kind, SettingKind::Font { .. }), "{font_key}");
        let size = setting(size_key).expect("a size row");
        let (min, max) = if size_key == VIEWER_FONT_SIZE_KEY {
            (6, 72)
        } else {
            (8, 24)
        };
        let SettingKind::Int {
            min: low,
            max: high,
            ..
        } = size.kind
        else {
            panic!("{size_key}: a number");
        };
        assert_eq!((low, high), (min, max), "{size_key}");
        assert_eq!(font.page, size.page, "{font_key}: one page");
    }
}
