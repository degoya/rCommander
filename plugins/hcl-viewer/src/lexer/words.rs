//! HCL's words and byte classes: the keywords of `for` expressions and template directives, the
//! constants, the type constraints, names and operators.

use crate::types::TokenClass;

/// The keywords: `for` expressions (`[for s in list : upper(s) if s != ""]`) and template
/// directives (`%{ if }`, `%{ else }`, `%{ endif }`, `%{ for }`, `%{ endfor }`). Sorted.
const KEYWORDS: &[&[u8]] = &[b"else", b"endfor", b"endif", b"for", b"if", b"in"];
/// The constants. Sorted.
const LITERALS: &[&[u8]] = &[b"false", b"null", b"true"];
/// The type constraints of a variable (`type = list(object({ name = string }))`). Sorted.
const TYPES: &[&[u8]] = &[
    b"any", b"bool", b"list", b"map", b"number", b"object", b"set", b"string", b"tuple",
];

/// The class of `word` alone: a keyword, a constant, a type, or none.
pub(crate) fn classify(word: &[u8]) -> Option<TokenClass> {
    let has = |table: &[&[u8]]| table.binary_search(&word).is_ok();
    if has(KEYWORDS) {
        Some(TokenClass::Keyword)
    } else if has(LITERALS) {
        Some(TokenClass::Literal)
    } else if has(TYPES) {
        Some(TokenClass::TypeName)
    } else {
        None
    }
}

/// A blank between tokens.
pub(crate) fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | 0x0B | 0x0C)
}

pub(crate) fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

/// A byte a name starts with: a letter, `_`, or part of a character beyond ASCII.
pub(crate) fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// A byte of a name: HCL's names take digits and `-` after the first (`aws_s3-bucket`).
pub(crate) fn is_name(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit() || byte == b'-'
}

/// The width of the operator at the start of `rest`, if one stands there.
pub(crate) fn operator_width(rest: &[u8]) -> Option<usize> {
    const TWO: [&[u8]; 7] = [b"==", b"!=", b"<=", b">=", b"&&", b"||", b"=>"];
    let pair = rest.get(..2);
    if pair.is_some_and(|pair| TWO.contains(&pair)) {
        return Some(2);
    }
    rest.first()
        .filter(|byte| b"=!+-*/%<>?:~".contains(*byte))
        .map(|_| 1)
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
