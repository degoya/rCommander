//! Windows' word sort for ASCII names in one pass of bytes (RC-110-93). An ASCII character has
//! no accent and no decomposition, so the first two passes of [`super::compare_names`] read such
//! a name alike, and the third - the one that sees hyphens and apostrophes - can only differ from
//! them where one was passed over. So two names compare here byte by byte, each byte through one
//! table that holds its class, its rank among the symbols and its case-folded value at once; the
//! hyphens are read only when the first pass found a tie and passed over one. A name with any
//! other character is handed back (`None`) once that character is reached - what the bytes before
//! it decided stands, as the first pass would decide it the same way.

use std::cmp::Ordering;

use super::SYMBOLS;

/// Every ASCII character's place in the word sort as one number: `class << 8 | rank`, the class
/// of [`super::class`] (symbols, digits, letters, hyphen and apostrophe), then the symbol's rank
/// of [`super::symbol_rank`], or the character case-folded for the others.
const KEYS: [u16; 128] = keys();

const fn keys() -> [u16; 128] {
    let mut keys = [0; 128];
    let mut at = 0;
    while at < keys.len() {
        let c = at as u8;
        keys[at] = if c == b'-' || c == b'\'' {
            (3 << 8) | c as u16
        } else if c.is_ascii_digit() {
            (1 << 8) | c as u16
        } else if c.is_ascii_alphabetic() {
            (2 << 8) | c.to_ascii_lowercase() as u16
        } else {
            symbol_rank(c)
        };
        at += 1;
    }
    keys
}

/// [`super::symbol_rank`] of the ASCII symbol `c`, at compile time.
const fn symbol_rank(c: u8) -> u16 {
    let symbols = SYMBOLS.as_bytes();
    let mut at = 0;
    while at < symbols.len() {
        if symbols[at] == c {
            return at as u16;
        }
        at += 1;
    }
    c as u16 + 128
}

/// `a` against `b` in Windows' word sort, runs of digits by their value with `numbers`; `None`
/// when a character that is not ASCII has to be read before the order is decided.
pub(super) fn compare(a: &str, b: &str, numbers: bool) -> Option<Ordering> {
    let (order, passed_over) = pass(a.as_bytes(), b.as_bytes(), true, numbers)?;
    if order != Ordering::Equal || !passed_over {
        return Some(order);
    }
    // Equal but for their hyphens and apostrophes: those decide now.
    pass(a.as_bytes(), b.as_bytes(), false, numbers).map(|(order, _)| order)
}

/// One pass over `a` and `b`, passing over hyphens and apostrophes with `skip`: the order, and
/// whether any was passed over; `None` at a character that is not ASCII.
fn pass(mut a: &[u8], mut b: &[u8], skip: bool, numbers: bool) -> Option<(Ordering, bool)> {
    let mut passed_over = false;
    loop {
        if skip {
            let before = a.len() + b.len();
            (a, b) = (skip_ignored(a), skip_ignored(b));
            passed_over |= a.len() + b.len() != before;
        }
        let (x, y) = (a.first().copied(), b.first().copied());
        if x.is_some_and(|x| !x.is_ascii()) || y.is_some_and(|y| !y.is_ascii()) {
            return None;
        }
        let (Some(x), Some(y)) = (x, y) else {
            // The shorter name first; equal when both ended.
            return Some((x.is_none().cmp(&y.is_none()).reverse(), passed_over));
        };
        if numbers && x.is_ascii_digit() && y.is_ascii_digit() {
            let (order, x_len, y_len) = digits_cmp(a, b);
            if order != Ordering::Equal {
                return Some((order, passed_over));
            }
            (a, b) = (&a[x_len..], &b[y_len..]);
            continue;
        }
        let order = KEYS[usize::from(x)].cmp(&KEYS[usize::from(y)]);
        if order != Ordering::Equal {
            return Some((order, passed_over));
        }
        (a, b) = (&a[1..], &b[1..]);
    }
}

/// `text` without its leading hyphens and apostrophes.
fn skip_ignored(text: &[u8]) -> &[u8] {
    let start = text
        .iter()
        .position(|&c| c != b'-' && c != b'\'')
        .unwrap_or(text.len());
    &text[start..]
}

/// The leading runs of digits of `a` and `b` by their value, as [`super::digits_cmp`] compares
/// them, and the length of each run.
fn digits_cmp(a: &[u8], b: &[u8]) -> (Ordering, usize, usize) {
    let (x_len, y_len) = (digits(a), digits(b));
    let (x, y) = (without_zeros(&a[..x_len]), without_zeros(&b[..y_len]));
    (x.len().cmp(&y.len()).then_with(|| x.cmp(y)), x_len, y_len)
}

/// How many ASCII digits `text` starts with.
fn digits(text: &[u8]) -> usize {
    text.iter()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(text.len())
}

/// `digits` without their leading zeros.
fn without_zeros(digits: &[u8]) -> &[u8] {
    let start = digits
        .iter()
        .position(|&c| c != b'0')
        .unwrap_or(digits.len());
    &digits[start..]
}

#[cfg(test)]
#[path = "ascii_tests.rs"]
mod tests;
