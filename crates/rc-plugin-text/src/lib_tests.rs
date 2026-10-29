use super::*;

#[test]
fn every_setting_value_names_its_encoding() {
    for (value, encoding) in [
        ("utf-8", Encoding::Utf8),
        ("utf-16le", Encoding::Utf16Le),
        ("utf-16be", Encoding::Utf16Be),
        ("windows-1252", Encoding::Windows1252),
        ("cp850", Encoding::Cp850),
        ("cp437", Encoding::Cp437),
    ] {
        assert_eq!(value.parse::<Encoding>(), Ok(encoding), "{value}");
    }
    for value in ["auto", "UTF-8", "latin-1", ""] {
        assert_eq!(value.parse::<Encoding>(), Err(UnknownEncoding), "{value}");
    }
}

#[test]
fn a_byte_order_mark_names_the_encoding_and_is_skipped() {
    assert_eq!(
        detect(b"\xEF\xBB\xBFkey=1", Encoding::Cp850),
        (Encoding::Utf8, 3)
    );
    assert_eq!(
        detect(b"\xFF\xFEW\x00", Encoding::Windows1252),
        (Encoding::Utf16Le, 2)
    );
    assert_eq!(
        detect(b"\xFE\xFF\x00W", Encoding::Windows1252),
        (Encoding::Utf16Be, 2)
    );
    // A byte-order mark wins over what follows it.
    assert_eq!(
        detect(b"\xEF\xBB\xBF\xE4", Encoding::Cp850),
        (Encoding::Utf8, 3)
    );
}

#[test]
fn without_a_mark_valid_utf8_is_utf8_else_the_fallback() {
    assert_eq!(
        detect("name = J\u{fc}rgen".as_bytes(), Encoding::Windows1252),
        (Encoding::Utf8, 0)
    );
    assert_eq!(detect(b"", Encoding::Windows1252), (Encoding::Utf8, 0));
    assert_eq!(
        detect(b"name = J\xFCrgen", Encoding::Windows1252),
        (Encoding::Windows1252, 0)
    );
    assert_eq!(detect(b"echo \x84", Encoding::Cp850), (Encoding::Cp850, 0));
    // The prefix cuts a character at its end: still UTF-8.
    assert_eq!(
        detect(b"price \xE2\x82", Encoding::Cp437),
        (Encoding::Utf8, 0)
    );
    // An invalid sequence before the end is not.
    assert_eq!(detect(b"\xE2\x82 x", Encoding::Cp437), (Encoding::Cp437, 0));
}
