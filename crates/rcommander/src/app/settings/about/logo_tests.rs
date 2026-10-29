use super::{ICON_PNG, LOGO_DARK_PNG, LOGO_PNG, decode, logo, window_icon};

#[test]
fn the_compiled_in_pictures_decode() {
    for png in [ICON_PNG, LOGO_PNG, LOGO_DARK_PNG] {
        assert!(decode(png).is_some());
    }
    assert!(window_icon().is_some());
    assert!(logo(false).is_some());
    assert!(logo(true).is_some());
}

#[test]
fn the_icon_is_square() {
    let icon = decode(ICON_PNG).expect("decodes");
    assert_eq!(icon.width(), icon.height());
}
