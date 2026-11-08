use super::*;

#[test]
fn track_and_disc_numbers_read_as_pairs() {
    assert_eq!(pair(&[0, 0, 0, 3, 0, 12, 0, 0]), "3/12");
    assert_eq!(pair(&[0, 0, 0, 3, 0, 0]), "3");
    assert_eq!(pair(&[0]), "1 bytes");
}

#[test]
fn values_read_by_their_type() {
    assert_eq!(value_of(1, b"Holiday", 7), "Holiday");
    assert_eq!(value_of(2, &[0, b'H', 0, b'i'], 4), "Hi");
    assert_eq!(value_of(21, &[0xFF, 0xFE], 2), "-2");
    assert_eq!(value_of(21, &[0x01, 0x00], 2), "256");
    assert_eq!(value_of(22, &[0xFF], 1), "255");
    assert_eq!(value_of(0, &[0, 1, 2], 3), "3 bytes");
    assert_eq!(value_of(0, b"plain", 5), "plain");
}

#[test]
fn quicktime_keys_and_items_read_as_words() {
    assert_eq!(
        key_label("com.apple.quicktime.location.ISO6709"),
        "Location"
    );
    assert_eq!(key_label("com.apple.quicktime.make"), "Make");
    assert_eq!(key_label("org.example.custom"), "org.example.custom");
    assert_eq!(item_label(b"\xa9nam"), "Title");
    assert_eq!(item_label(b"zzzz"), "zzzz");
    assert_eq!(item_label(b"\xa9zzz"), "\u{a9}zzz");
}

#[test]
fn keyed_items_name_their_key() {
    let keys = vec!["com.apple.quicktime.make".to_owned()];
    assert_eq!(keyed(&1u32.to_be_bytes(), &keys), Some(&keys[0]));
    assert_eq!(keyed(&2u32.to_be_bytes(), &keys), None);
    assert_eq!(keyed(&0u32.to_be_bytes(), &keys), None);
}
