use super::*;

/// The menus follow the app's look, whichever theme chose it.
#[test]
fn the_mode_follows_the_look() {
    assert_eq!(mode(true), AppMode::ForceDark);
    assert_eq!(mode(false), AppMode::ForceLight);
    assert_eq!(AppMode::ForceDark as i32, 2, "uxtheme's PreferredAppMode");
    assert_eq!(AppMode::ForceLight as i32, 3);
}

/// Ordinal 135 is `SetPreferredAppMode` from build 18362 on; before, it was
/// `AllowDarkModeForApp`, and an unknown build is left alone too.
#[test]
fn only_builds_from_1903_on_are_asked() {
    assert!(supported(Some(18362)));
    assert!(supported(Some(26100)));
    assert!(!supported(Some(18361)));
    assert!(
        !supported(Some(17763)),
        "1809: ordinal 135 is AllowDarkModeForApp"
    );
    assert!(!supported(None));
}

#[test]
fn the_build_number_reads_from_the_registry_text() {
    assert_eq!(build_number("22631"), Some(22631));
    assert_eq!(build_number(" 19045 \n"), Some(19045));
    assert_eq!(build_number(""), None);
    assert_eq!(build_number("10.0"), None);
}
