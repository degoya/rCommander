use super::*;

fn decode(bytes: &[u8], encoding: Encoding) -> String {
    let mut out = String::new();
    let mut decoder = Decoder::new(encoding);
    decoder.push(bytes, &mut out);
    decoder.finish(&mut out);
    out
}

/// `bytes` decoded in two windows cut at every place, each compared with `expected`.
fn assert_every_cut(bytes: &[u8], encoding: Encoding, expected: &str) {
    for cut in 0..=bytes.len() {
        let mut out = String::new();
        let mut decoder = Decoder::new(encoding);
        decoder.push(&bytes[..cut], &mut out);
        decoder.push(&bytes[cut..], &mut out);
        decoder.finish(&mut out);
        assert_eq!(out, expected, "{encoding:?}, {bytes:?} cut at {cut}");
    }
    let mut out = String::new();
    let mut decoder = Decoder::new(encoding);
    for byte in bytes {
        decoder.push(std::slice::from_ref(byte), &mut out);
    }
    decoder.finish(&mut out);
    assert_eq!(out, expected, "{encoding:?}, {bytes:?} a byte at a time");
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn utf16be(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

#[test]
fn utf8_stays_utf8_also_cut_by_windows() {
    let text = "key = \u{e4}\u{f6}\u{fc} \u{20ac} \u{1f600}\r\n";
    assert_eq!(decode(text.as_bytes(), Encoding::Utf8), text);
    assert_every_cut(text.as_bytes(), Encoding::Utf8, text);
}

#[test]
fn invalid_utf8_reads_as_from_utf8_lossy_does() {
    for bytes in [
        &b"a\xE4b"[..],
        b"\x84\x9B",
        b"a\xE4\xB8\nb",
        b"end \xF0\x9F\x98",
        b"\xC3",
        b"\xED\xA0\x80 surrogate",
        b"\xC0\xAF overlong",
        b"\xF4\x90\x80\x80 above",
        b"\xE2\x41\x41",
    ] {
        let expected = String::from_utf8_lossy(bytes);
        assert_eq!(decode(bytes, Encoding::Utf8), expected, "{bytes:?}");
        assert_every_cut(bytes, Encoding::Utf8, &expected);
    }
}

#[test]
fn utf16_in_both_byte_orders_also_cut_by_windows() {
    // A surrogate pair (the emoji) cut between its halves and inside each.
    let text = "[HKEY_CURRENT_USER\\x]\r\n\"N\u{e4}me\"=\"\u{20ac} \u{1f600}\"\r\n";
    assert_eq!(decode(&utf16le(text), Encoding::Utf16Le), text);
    assert_eq!(decode(&utf16be(text), Encoding::Utf16Be), text);
    assert_every_cut(&utf16le(text), Encoding::Utf16Le, text);
    assert_every_cut(&utf16be(text), Encoding::Utf16Be, text);
}

#[test]
fn unpaired_surrogates_and_an_odd_byte_are_replaced() {
    // A lone high surrogate before a letter, a lone low one, a high one at the end.
    let units: [u16; 5] = [0xD83D, 0x0061, 0xDE00, 0x0062, 0xD83D];
    let expected = String::from_utf16_lossy(&units);
    assert_eq!(expected, "\u{fffd}a\u{fffd}b\u{fffd}");
    let le: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    let be: Vec<u8> = units.iter().flat_map(|unit| unit.to_be_bytes()).collect();
    assert_every_cut(&le, Encoding::Utf16Le, &expected);
    assert_every_cut(&be, Encoding::Utf16Be, &expected);
    // An odd byte at the end of the file.
    assert_every_cut(b"a\x00b", Encoding::Utf16Le, "a\u{fffd}");
    assert_every_cut(b"\x00a\x00", Encoding::Utf16Be, "a\u{fffd}");
}

#[test]
fn cp850_reads_german_umlauts() {
    assert_eq!(
        decode(&[0x84, 0x94, 0x81, 0xE1], Encoding::Cp850),
        "\u{e4}\u{f6}\u{fc}\u{df}"
    );
    assert_eq!(
        decode(b"echo \x8E\x99\x9A \xB0\xC9\xD1", Encoding::Cp850),
        "echo \u{c4}\u{d6}\u{dc} \u{2591}\u{2554}\u{d0}"
    );
}

#[test]
fn cp437_shares_the_umlauts_and_differs_above() {
    assert_eq!(
        decode(&[0x84, 0x94, 0x81, 0xE1, 0xB0, 0xC9], Encoding::Cp437),
        "\u{e4}\u{f6}\u{fc}\u{df}\u{2591}\u{2554}"
    );
    // 0x9B and 0xD1: a cent sign and a box line in 437, a slashed o and an eth in 850.
    assert_eq!(decode(&[0x9B, 0xD1], Encoding::Cp437), "\u{a2}\u{2564}");
    assert_eq!(decode(&[0x9B, 0xD1], Encoding::Cp850), "\u{f8}\u{d0}");
}

#[test]
fn windows_1252_reads_latin_1_and_its_own_upper_half() {
    assert_eq!(
        decode(&[0xE4, 0xF6, 0xFC, 0xDF, 0x80, 0x9F], Encoding::Windows1252),
        "\u{e4}\u{f6}\u{fc}\u{df}\u{20ac}\u{178}"
    );
    // Excel's quotes, and the five undefined bytes as the C1 controls of their number.
    assert_eq!(
        decode(b"\x93q\x94", Encoding::Windows1252),
        "\u{201c}q\u{201d}"
    );
    assert_eq!(
        decode(&[0x81, 0x8D, 0x8F, 0x90, 0x9D], Encoding::Windows1252),
        "\u{81}\u{8d}\u{8f}\u{90}\u{9d}"
    );
}

#[test]
fn ascii_is_ascii_in_every_byte_encoding() {
    let ascii: Vec<u8> = (0..0x80).collect();
    let expected: String = ascii.iter().map(|byte| char::from(*byte)).collect();
    for encoding in [
        Encoding::Utf8,
        Encoding::Windows1252,
        Encoding::Cp850,
        Encoding::Cp437,
    ] {
        assert_eq!(decode(&ascii, encoding), expected, "{encoding:?}");
        assert_every_cut(&ascii, encoding, &expected);
    }
}

#[test]
fn every_byte_of_a_code_page_is_one_non_ascii_character() {
    let high: Vec<u8> = (0x80..=0xFF).collect();
    for encoding in [Encoding::Windows1252, Encoding::Cp850, Encoding::Cp437] {
        let text = decode(&high, encoding);
        assert_eq!(text.chars().count(), 128, "{encoding:?}");
        assert!(text.chars().all(|c| !c.is_ascii()), "{encoding:?}");
    }
    assert!(decode(&high, Encoding::Utf8).chars().all(|c| !c.is_ascii()));
}

/// RC-080-52: only the cut character is put together; the windows around it decode as they are.
#[test]
fn a_character_split_over_two_windows_is_put_together() {
    let mut decoder = Decoder::new(Encoding::Utf8);
    let mut out = String::new();
    decoder.push(b"price \xE2", &mut out);
    assert_eq!(out, "price ");
    decoder.push(b"\x82", &mut out);
    decoder.push(b"\xAC and \xF0\x9F", &mut out);
    decoder.push(b"\x98\x80!", &mut out);
    decoder.push(b"\xE2", &mut out);
    decoder.push(b"A", &mut out);
    decoder.finish(&mut out);
    assert_eq!(out, "price \u{20ac} and \u{1f600}!\u{fffd}A");
}

#[test]
fn finish_starts_the_decoder_over() {
    let mut decoder = Decoder::new(Encoding::Utf8);
    let mut out = String::new();
    decoder.push(b"a\xE2\x82", &mut out);
    decoder.finish(&mut out);
    decoder.push(b"\xAC", &mut out);
    decoder.finish(&mut out);
    assert_eq!(out, "a\u{fffd}\u{fffd}");
    let mut empty = String::new();
    Decoder::new(Encoding::Utf16Le).finish(&mut empty);
    assert_eq!(empty, "");
}
