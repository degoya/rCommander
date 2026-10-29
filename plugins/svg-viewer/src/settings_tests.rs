use super::*;

#[test]
fn size_reads_each_value_and_defaults_to_1024() {
    assert_eq!(Size::from_setting(Some("512")).longest(), 512);
    assert_eq!(Size::from_setting(Some("1024")).longest(), 1024);
    assert_eq!(Size::from_setting(Some("2048")).longest(), 2048);
    assert_eq!(Size::from_setting(None), Size::Medium);
    assert_eq!(Size::from_setting(Some("4096")), Size::Medium);
}

#[test]
fn background_reads_each_value_and_defaults_to_transparent() {
    assert_eq!(
        Background::from_setting(Some("white")).color(),
        Some(Color::WHITE)
    );
    assert_eq!(
        Background::from_setting(Some("black")).color(),
        Some(Color::BLACK)
    );
    assert_eq!(Background::from_setting(Some("transparent")).color(), None);
    assert_eq!(Background::from_setting(None), Background::Transparent);
    assert_eq!(
        Background::from_setting(Some("red")),
        Background::Transparent
    );
}

#[test]
fn settings_take_both_values() {
    assert_eq!(
        Settings::from_values(Some("2048"), Some("black")),
        Settings {
            size: Size::Large,
            background: Background::Black,
        }
    );
    assert_eq!(Settings::from_values(None, None), Settings::default());
}
