use super::*;

fn decode(bytes: &[u8], encoding: Encoding) -> String {
    let mut out = String::new();
    decode_line(bytes, false, encoding, &mut out);
    out
}

// The code pages themselves are `rc-plugin-text`'s, tested there; these hold the setting to them.
#[test]
fn each_setting_reads_its_code_page() {
    assert_eq!(
        decode(b"echo \x84\x94\x81\xE1", Encoding::Cp850),
        "echo \u{e4}\u{f6}\u{fc}\u{df}"
    );
    // 0x9B and 0xD1: a cent sign and a box line in 437, a slashed o and an eth in 850.
    assert_eq!(decode(&[0x9B, 0xD1], Encoding::Cp437), "\u{a2}\u{2564}");
    assert_eq!(decode(&[0x9B, 0xD1], Encoding::Cp850), "\u{f8}\u{d0}");
    assert_eq!(
        decode(&[0xE4, 0x80], Encoding::Windows1252),
        "\u{e4}\u{20ac}"
    );
    assert_eq!(decode(b"a\xE4b", Encoding::Utf8), "a\u{fffd}b");
}

#[test]
fn every_non_ascii_byte_becomes_non_ascii_text() {
    let high: Vec<u8> = (0x80..=0xFF).collect();
    for encoding in [
        Encoding::Utf8,
        Encoding::Cp850,
        Encoding::Cp437,
        Encoding::Windows1252,
    ] {
        assert!(
            decode(&high, encoding).chars().all(|c| !c.is_ascii()),
            "{encoding:?}"
        );
    }
}

#[test]
fn a_capped_line_leaves_a_cut_sequence_out() {
    let mut out = String::new();
    decode_line(b"a\xC3", true, Encoding::Utf8, &mut out);
    assert_eq!(out, "a");
    out.clear();
    decode_line(b"a\xC3", false, Encoding::Utf8, &mut out);
    assert_eq!(out, "a\u{fffd}");
    out.clear();
    decode_line(b"a\x84", true, Encoding::Cp850, &mut out);
    assert_eq!(out, "a\u{e4}");
}

#[test]
fn the_utf8_check_follows_windows() {
    let text = "a\u{e4}\u{20ac}\u{1f600}z".as_bytes();
    for cut in 0..text.len() {
        let mut check = Utf8Check::default();
        check.feed(&text[..cut]);
        check.feed(&text[cut..]);
        assert!(check.valid(), "cut at {cut}");
    }
    let mut check = Utf8Check::default();
    for byte in text {
        check.feed(std::slice::from_ref(byte));
    }
    assert!(check.valid());

    let mut cut = Utf8Check::default();
    cut.feed(b"end \xE2\x82");
    assert!(!cut.valid(), "a sequence cut at the end");
    let mut invalid = Utf8Check::default();
    invalid.feed(b"echo \x84");
    invalid.feed(b"more");
    assert!(!invalid.valid());
    let mut bad_continuation = Utf8Check::default();
    bad_continuation.feed(b"\xE2");
    bad_continuation.feed(b"\x41\x41");
    assert!(!bad_continuation.valid());
    assert!(Utf8Check::default().valid());
}

#[test]
fn every_setting_value_is_read() {
    for (value, encoding) in [
        (Some("auto"), Encoding::Auto),
        (Some("utf-8"), Encoding::Utf8),
        (Some("cp850"), Encoding::Cp850),
        (Some("cp437"), Encoding::Cp437),
        (Some("windows-1252"), Encoding::Windows1252),
        (Some("latin-9"), Encoding::Auto),
        (None, Encoding::Auto),
    ] {
        assert_eq!(Encoding::from_setting(value), encoding, "{value:?}");
    }
}
