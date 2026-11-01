//! The two classes and the menu entries that name them.

use super::{MAIN, POPUP};

const MAIN_ENTRY: &str = include_str!("../../../../packaging/linux/rcommander.desktop");
const POPUP_ENTRY: &str = include_str!("../../../../packaging/linux/rcommander-popup.desktop");

/// The value of `key` in a desktop entry, if it has one.
fn value<'a>(entry: &'a str, key: &str) -> Option<&'a str> {
    entry
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
}

#[test]
fn the_main_window_and_the_dialogs_have_different_classes() {
    assert_eq!(MAIN, "rcommander");
    assert_eq!(POPUP, "rcommander-popup");
}

#[test]
fn each_menu_entry_names_its_window_class() {
    assert_eq!(value(MAIN_ENTRY, "StartupWMClass"), Some(MAIN));
    assert_eq!(value(POPUP_ENTRY, "StartupWMClass"), Some(POPUP));
}

#[test]
fn the_popup_entry_is_hidden_and_carries_the_app_icon() {
    assert_eq!(value(POPUP_ENTRY, "NoDisplay"), Some("true"));
    assert_eq!(value(POPUP_ENTRY, "Icon"), value(MAIN_ENTRY, "Icon"));
}

#[cfg(target_os = "linux")]
#[test]
fn the_class_becomes_the_application_id() {
    let settings = super::with_class(iced::window::Settings::default(), POPUP);
    assert_eq!(settings.platform_specific.application_id, POPUP);
}

#[cfg(not(target_os = "linux"))]
#[test]
fn elsewhere_the_settings_stay_as_they_were() {
    let settings = super::with_class(iced::window::Settings::default(), POPUP);
    assert_eq!(
        settings.platform_specific,
        iced::window::Settings::default().platform_specific
    );
}
