use super::*;

#[test]
fn cover_is_on_unless_switched_off() {
    assert_eq!(Cover::from_setting(None), Cover::On);
    assert_eq!(Cover::from_setting(Some("on")), Cover::On);
    assert_eq!(Cover::from_setting(Some("off")), Cover::Off);
    assert_eq!(Cover::from_setting(Some("sometimes")), Cover::On);
}

#[test]
fn settings_read_their_values() {
    assert_eq!(Settings::from_values(Some("off")).cover, Cover::Off);
    assert_eq!(Settings::default().cover, Cover::On);
}
