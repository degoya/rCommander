use super::*;

fn valid(bytes: &[u8]) -> bool {
    let mut check = Utf8Check::default();
    for byte in bytes {
        check.feed(*byte);
    }
    check.valid()
}

#[test]
fn windows_1252_reads_umlauts_and_the_euro_sign() {
    // "Gr\xfc\xdfe 5 \x80" as Excel writes it on a German Windows.
    let bytes = b"Gr\xfc\xdfe \xc4\xd6 5 \x80 \x93q\x94";
    assert_eq!(
        decode(bytes, Encoding::Windows1252),
        "Gr\u{fc}\u{df}e \u{c4}\u{d6} 5 \u{20ac} \u{201c}q\u{201d}"
    );
    let mut out = Vec::new();
    decode_into(bytes, Encoding::Windows1252, &mut out);
    assert_eq!(
        out,
        decode(bytes, Encoding::Windows1252).into_bytes(),
        "both decoders agree"
    );
    // The five bytes WHATWG leaves undefined are the C1 controls of the same number.
    assert_eq!(decode(b"\x81\x9da", Encoding::Windows1252), "\u{81}\u{9d}a");
}

#[test]
fn utf8_is_checked_byte_by_byte() {
    assert!(valid("a\u{e4}\u{20ac}\u{1f600}".as_bytes()));
    assert!(valid(b""));
    for bad in [
        &b"\xfc"[..],        // Windows-1252's u-umlaut
        b"\xc3",             // cut at the end
        b"\xc0\xaf",         // overlong
        b"\xed\xa0\x80",     // a surrogate
        b"\xf4\x90\x80\x80", // above U+10FFFF
        b"\xe2\x82",         // cut
        b"a\x80",            // a lone continuation byte
    ] {
        assert!(!valid(bad), "{bad:?}");
    }
}

#[test]
fn utf8_that_is_not_valid_becomes_u_fffd() {
    let bytes = b"a\xffb\xe2\x82";
    assert_eq!(decode(bytes, Encoding::Utf8), "a\u{fffd}b\u{fffd}");
    assert_eq!(
        decode(bytes, Encoding::Utf8),
        String::from_utf8_lossy(bytes)
    );
    let mut out = Vec::new();
    decode_into("\u{e4}\u{20ac}".as_bytes(), Encoding::Utf8, &mut out);
    assert_eq!(out, "\u{e4}\u{20ac}".as_bytes());
}
