//! The pieces of a line the lexer recognises by their bytes alone: names, numbers and operators.
//! Every decision is on ASCII; a byte of 0x80 and above is part of a name, so no piece starts or
//! ends inside a character.

use crate::lang::Lang;

/// A byte of a name: a letter, a digit, `_`, part of a character beyond ASCII, and `$` where
/// the language allows it.
pub(crate) fn is_name(lang: Lang, byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || byte == b'_'
        || byte >= 0x80
        || (byte == b'$' && lang.dollar_in_names())
}

/// A byte a name starts with.
pub(crate) fn is_name_start(lang: Lang, byte: u8) -> bool {
    is_name(lang, byte) && !byte.is_ascii_digit()
}

/// Blank: a space, a tab, a vertical tab, a form feed, a stray `\r`.
pub(crate) fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0B | 0x0C | b'\r')
}

/// The end of the name run from `at`.
pub(crate) fn name_end(lang: Lang, line: &[u8], at: usize) -> usize {
    at + line
        .get(at..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| is_name(lang, **byte))
        .count()
}

/// The first byte from `at` that is not blank.
pub(crate) fn skip_blanks(line: &[u8], at: usize) -> usize {
    at + line
        .get(at..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| is_blank(**byte))
        .count()
}

/// The end of the number at `at`, a digit or a `.` before one, with every prefix and suffix the
/// family writes: `42`, `0xFF`, `0b1010`, `0o17`, `1_000`, `1'000` (C, C++), `1.5`, `.5`,
/// `6.02e+23`, `0x1p-3`, `10u`, `1.5f`, `100L`, `2.0m`, `255u8`, `1e3_f64`. After a `.`
/// (`member`) a number is a tuple's field and takes no fraction: `pair.0.1` is two fields.
pub(crate) fn number_end(lang: Lang, line: &[u8], at: usize, member: bool) -> usize {
    let hex = line.get(at) == Some(&b'0') && matches!(line.get(at + 1), Some(b'x' | b'X'));
    let digit_follows = |end: usize| line.get(end).is_some_and(u8::is_ascii_digit);
    let mut end = at;
    let mut dot = member;
    if line.get(at) == Some(&b'.') {
        end += 1;
        dot = true;
    }
    while let Some(&byte) = line.get(end) {
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            end += 1;
            let exponent = if hex {
                matches!(byte, b'p' | b'P')
            } else {
                matches!(byte, b'e' | b'E')
            };
            if exponent && matches!(line.get(end), Some(b'+' | b'-')) && digit_follows(end + 1) {
                end += 1;
            }
        } else if byte == b'.' && !dot && !hex && digit_follows(end + 1) {
            dot = true;
            end += 1;
        } else if byte == b'\''
            && lang.digit_quotes()
            && line.get(end + 1).is_some_and(u8::is_ascii_alphanumeric)
        {
            end += 1;
        } else {
            break;
        }
    }
    end
}

/// The operators of four, three and two bytes, longest first; one-byte ones are
/// [`ONE_BYTE_OPERATORS`]. `::` is punctuation, not here.
const OPERATORS: &[&[u8]] = &[
    b">>>=", b"<<=", b">>=", b">>>", b"...", b"..=", b"..<", b"<=>", b"->*", b"&^=", b"===",
    b"!==", b"->", b"=>", b"==", b"!=", b"<=", b">=", b"&&", b"||", b"++", b"--", b"+=", b"-=",
    b"*=", b"/=", b"%=", b"&=", b"|=", b"^=", b"<<", b">>", b"??", b"?.", b"?:", b"!!", b":=",
    b"<-", b"&^", b"..", b".*", b"##",
];
const ONE_BYTE_OPERATORS: &[u8] = b"+-*/%&|^~<>=!?@#";

/// The width of the operator `rest` starts with, if it starts with one.
pub(crate) fn operator_width(rest: &[u8]) -> Option<usize> {
    OPERATORS
        .iter()
        .find(|operator| rest.starts_with(operator))
        .map(|operator| operator.len())
        .or_else(|| {
            rest.first()
                .filter(|byte| ONE_BYTE_OPERATORS.contains(byte))
                .map(|_| 1)
        })
}

/// The length of the run of `byte` from `at`, at most 255.
pub(crate) fn run_of(line: &[u8], at: usize, byte: u8) -> usize {
    line.get(at..)
        .unwrap_or_default()
        .iter()
        .take(usize::from(u8::MAX))
        .take_while(|other| **other == byte)
        .count()
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
