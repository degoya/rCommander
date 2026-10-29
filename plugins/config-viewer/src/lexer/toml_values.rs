//! The classes of TOML's bare values: `true` and `false` and dates and times are `literal`,
//! numbers `number`, anything else `invalid`.

use crate::types::TokenClass;

pub(super) fn class(word: &[u8]) -> TokenClass {
    if matches!(word, b"true" | b"false") || is_date_time(word) {
        TokenClass::Literal
    } else if is_number(word) {
        TokenClass::Number
    } else {
        TokenClass::Invalid
    }
}

/// A full date, `1979-05-27`, alone.
pub(super) fn is_date(word: &[u8]) -> bool {
    word.len() == 10 && starts_with_date(word)
}

fn digits(word: &[u8], range: std::ops::Range<usize>) -> bool {
    word.get(range)
        .is_some_and(|part| part.iter().all(u8::is_ascii_digit))
}

fn starts_with_date(word: &[u8]) -> bool {
    digits(word, 0..4)
        && word.get(4) == Some(&b'-')
        && digits(word, 5..7)
        && word.get(7) == Some(&b'-')
        && digits(word, 8..10)
}

/// An offset or local date-time, a local date or a local time: `1979-05-27T07:32:00Z`,
/// `1979-05-27 07:32:00.999-07:00`, `1979-05-27`, `07:32:00`.
fn is_date_time(word: &[u8]) -> bool {
    let time = digits(word, 0..2) && word.get(2) == Some(&b':') && digits(word, 3..5);
    (starts_with_date(word) || time)
        && word.iter().all(|byte| {
            byte.is_ascii_digit()
                || matches!(
                    byte,
                    b'-' | b':' | b'.' | b'T' | b't' | b'Z' | b'z' | b'+' | b' '
                )
        })
}

/// An integer (decimal, `0x`, `0o`, `0b`, underscores between digits), a float, `inf` or `nan`.
fn is_number(word: &[u8]) -> bool {
    if let Some(hex) = word.strip_prefix(b"0x") {
        return radix(hex, u8::is_ascii_hexdigit);
    }
    if let Some(octal) = word.strip_prefix(b"0o") {
        return radix(octal, |byte| (b'0'..=b'7').contains(byte));
    }
    if let Some(binary) = word.strip_prefix(b"0b") {
        return radix(binary, |byte| matches!(byte, b'0' | b'1'));
    }
    let unsigned = word
        .strip_prefix(b"+")
        .or_else(|| word.strip_prefix(b"-"))
        .unwrap_or(word);
    if matches!(unsigned, b"inf" | b"nan") {
        return true;
    }
    let (mantissa, exponent) = match unsigned.iter().position(|byte| matches!(byte, b'e' | b'E')) {
        Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.iter().position(|byte| *byte == b'.') {
        Some(at) => (&mantissa[..at], Some(&mantissa[at + 1..])),
        None => (mantissa, None),
    };
    let exponent = exponent.map(|part| {
        part.strip_prefix(b"+")
            .or_else(|| part.strip_prefix(b"-"))
            .unwrap_or(part)
    });
    separated(whole) && fraction.is_none_or(separated) && exponent.is_none_or(separated)
}

/// Digits with single underscores between them.
fn separated(part: &[u8]) -> bool {
    radix(part, u8::is_ascii_digit)
}

fn radix(part: &[u8], digit: impl Fn(&u8) -> bool) -> bool {
    part.first().is_some_and(&digit)
        && part.last().is_some_and(&digit)
        && part.iter().all(|byte| *byte == b'_' || digit(byte))
        && !part.windows(2).any(|pair| pair == b"__")
}

#[cfg(test)]
#[path = "toml_values_tests.rs"]
mod tests;
