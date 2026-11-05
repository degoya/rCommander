//! The counter-checks of `encodings.rs`: a canary hidden in each form, inside other text, is
//! found; text without it is not.

use super::*;

/// Standard base64 of a whole stream, padding left out: how a leak would be written.
fn base64_stream(bytes: &[u8]) -> String {
    let mut text = base64_groups(bytes);
    let tail = &bytes[bytes.len() - bytes.len() % 3..];
    if !tail.is_empty() {
        let mut padded = [0_u8; 3];
        padded[..tail.len()].copy_from_slice(tail);
        let group = base64_groups(&padded);
        text.push_str(&group[..=tail.len()]);
    }
    text
}

const CANARY: &str = "canary-sweep-check+/=91";

fn needles() -> Needles {
    Needles::of(&[CANARY])
}

#[test]
fn plain_and_hex_in_both_cases_are_found() {
    let needles = needles();
    for leak in [
        format!("before {CANARY} after"),
        format!("0x{}!", hex(CANARY.as_bytes(), false)),
        format!("0x{}!", hex(CANARY.as_bytes(), true)),
    ] {
        assert!(!needles.found(leak.as_bytes()).is_empty(), "{leak}");
    }
    assert!(needles.found(b"nothing of it here").is_empty());
}

#[test]
fn base64_is_found_from_every_offset() {
    let needles = needles();
    for offset in 0..6 {
        let mut stream = vec![b'x'; offset];
        stream.extend_from_slice(CANARY.as_bytes());
        stream.extend_from_slice(b"tail");
        let leak = base64_stream(&stream);
        assert!(
            !needles.found(leak.as_bytes()).is_empty(),
            "offset {offset}"
        );
    }
    let unrelated = base64_stream(b"an unrelated stream of bytes, long enough");
    assert!(needles.found(unrelated.as_bytes()).is_empty());
}

#[test]
fn url_encoding_is_found_in_both_cases() {
    let needles = needles();
    for upper in [false, true] {
        let leak = format!("?q={}&x=1", url_encoded(CANARY.as_bytes(), upper));
        assert!(leak.contains('%'), "the canary has reserved characters");
        assert!(!needles.found(leak.as_bytes()).is_empty(), "{leak}");
    }
}

#[test]
fn utf16_is_found_in_both_byte_orders() {
    let needles = needles();
    for big_endian in [false, true] {
        let text = format!("<{CANARY}>");
        let units = text.encode_utf16();
        let leak: Vec<u8> = if big_endian {
            units.flat_map(u16::to_be_bytes).collect()
        } else {
            units.flat_map(u16::to_le_bytes).collect()
        };
        assert!(!needles.found(&leak).is_empty(), "big endian {big_endian}");
    }
}

#[test]
fn a_needle_survives_invalid_bytes_around_it() {
    let mut leak = vec![0xff, 0xfe, 0xc3];
    leak.extend_from_slice(CANARY.as_bytes());
    leak.push(0x80);
    assert_eq!(needles().found(&leak), vec![CANARY.to_owned()]);
}
