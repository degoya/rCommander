//! The flat formats: `.env` (`KEY=value`, `export KEY=value`, quoted values that may go on over
//! lines, `#` comments) and Java properties (`key=value`, `key: value` or `key value`, `#` and
//! `!` comments, a `\` at a line's end continuing the value).

use super::{LineState, Out, closing, comment_start, rest, scalar_class, skip_space, trim_end};
use crate::types::TokenClass;

const COMMENT: &[u8] = b"#";

/// A line of a `.env` file.
pub(super) fn env(text: &[u8], state: LineState, out: &mut Out<'_>) -> LineState {
    if let LineState::Quoted(quote) = state {
        return quoted(text, 0, 0, quote, out);
    }
    let mut at = skip_space(text, 0);
    match text.get(at) {
        None => return LineState::Clear,
        Some(b'#') => {
            out.push(at, text.len(), TokenClass::Comment);
            return LineState::Clear;
        }
        Some(_) => {}
    }
    if text
        .get(at..)
        .is_some_and(|tail| tail.starts_with(b"export "))
        || text
            .get(at..)
            .is_some_and(|tail| tail.starts_with(b"export\t"))
    {
        out.push(at, at + 6, TokenClass::Keyword);
        at = skip_space(text, at + 6);
    }
    let key_end = at
        + text
            .get(at..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
            .count();
    if key_end == at {
        out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid);
        return LineState::Clear;
    }
    out.push(at, key_end, TokenClass::Key);
    let equals = skip_space(text, key_end);
    match text.get(equals) {
        // `export NAME` alone.
        None => return LineState::Clear,
        Some(b'=') => out.push(equals, equals + 1, TokenClass::Punctuation),
        Some(_) => {
            rest(text, equals, COMMENT, out);
            return LineState::Clear;
        }
    }
    let at = skip_space(text, equals + 1);
    match text.get(at) {
        None => LineState::Clear,
        Some(&quote @ (b'"' | b'\'' | b'`')) => quoted(text, at, at + 1, quote, out),
        Some(_) => {
            let end = comment_start(text, at, COMMENT).unwrap_or(text.len());
            let trimmed = trim_end(text, at, end);
            out.push(at, trimmed, scalar_class(&text[at..trimmed]));
            out.push(end, text.len(), TokenClass::Comment);
            LineState::Clear
        }
    }
}

/// A quoted value from `start`, its closing quote searched from `from`: a string to the quote and
/// a comment after it, or to the line's end and on in the next line.
fn quoted(text: &[u8], start: usize, from: usize, quote: u8, out: &mut Out<'_>) -> LineState {
    match closing(text, from, quote, quote == b'"') {
        Some(end) => {
            out.push(start, end, TokenClass::String);
            rest(text, end, COMMENT, out);
            LineState::Clear
        }
        None => {
            out.push(start, text.len(), TokenClass::String);
            LineState::Quoted(quote)
        }
    }
}

/// A line of a properties file.
pub(super) fn properties(text: &[u8], state: LineState, out: &mut Out<'_>) -> LineState {
    let at = skip_space(text, 0);
    if state == LineState::Continued {
        out.push(at, text.len(), TokenClass::String);
        return continues(text);
    }
    match text.get(at) {
        None => return LineState::Clear,
        Some(b'#' | b'!') => {
            out.push(at, text.len(), TokenClass::Comment);
            return LineState::Clear;
        }
        Some(_) => {}
    }
    // The key: up to an `=`, `:` or space no backslash escapes.
    let mut end = at;
    while let Some(&byte) = text.get(end) {
        if byte == b'\\' {
            end += 2;
            continue;
        }
        if matches!(byte, b'=' | b':' | b' ' | b'\t' | b'\x0C') {
            break;
        }
        end += 1;
    }
    let end = end.min(text.len());
    out.push(at, end, TokenClass::Key);
    let mut value = skip_space(text, end);
    if matches!(text.get(value), Some(b'=' | b':')) {
        out.push(value, value + 1, TokenClass::Punctuation);
        value = skip_space(text, value + 1);
    }
    let next = continues(text);
    if value < text.len() {
        let class = if next == LineState::Continued {
            TokenClass::String
        } else {
            scalar_class(&text[value..trim_end(text, value, text.len())])
        };
        out.push(value, text.len(), class);
    }
    next
}

/// `Continued` when the line ends in an odd number of backslashes, else `Clear`.
fn continues(text: &[u8]) -> LineState {
    let backslashes = text.iter().rev().take_while(|byte| **byte == b'\\').count();
    if backslashes % 2 == 1 {
        LineState::Continued
    } else {
        LineState::Clear
    }
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
