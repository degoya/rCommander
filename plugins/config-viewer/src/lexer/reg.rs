//! A Windows registry export (`.reg`), as regedit writes it: the version line, `[HKEY_...]` keys
//! (`[-HKEY_...]` deletes one), `"Name"=` and `@=` values - a string, `-` (deleted),
//! `dword:0000001f`, or `hex:`/`hex(2):` bytes, which a `\` continues on the next line - and `;`
//! comments.

use super::{LineState, Out, closing, ini, rest, skip_space, trim_end};
use crate::types::TokenClass;

const COMMENT: &[u8] = b";";

pub(super) fn reg(text: &[u8], state: LineState, out: &mut Out<'_>) -> LineState {
    let at = skip_space(text, 0);
    if state == LineState::Continued {
        return hex_bytes(text, at, out);
    }
    match text.get(at) {
        None => {}
        Some(b';') => out.push(at, text.len(), TokenClass::Comment),
        Some(b'[') => ini::header(text, at, COMMENT, out),
        Some(b'"' | b'@') => return value_line(text, at, out),
        Some(_) => {
            let end = trim_end(text, at, text.len());
            let line = text.get(at..end).unwrap_or_default();
            let class =
                if line.starts_with(b"Windows Registry Editor Version") || line == b"REGEDIT4" {
                    TokenClass::Keyword
                } else {
                    TokenClass::Invalid
                };
            out.push(at, end, class);
        }
    }
    LineState::Clear
}

/// `"Name"=value` or `@=value`.
fn value_line(text: &[u8], at: usize, out: &mut Out<'_>) -> LineState {
    let name_end = if text.get(at) == Some(&b'@') {
        Some(at + 1)
    } else {
        closing(text, at + 1, b'"', true)
    };
    let Some(name_end) = name_end else {
        out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid);
        return LineState::Clear;
    };
    out.push(at, name_end, TokenClass::Key);
    let equals = skip_space(text, name_end);
    if text.get(equals) != Some(&b'=') {
        rest(text, equals, COMMENT, out);
        return LineState::Clear;
    }
    out.push(equals, equals + 1, TokenClass::Punctuation);
    let at = skip_space(text, equals + 1);
    match text.get(at) {
        None => {}
        Some(b'"') => match closing(text, at + 1, b'"', true) {
            Some(end) => {
                out.push(at, end, TokenClass::String);
                rest(text, end, COMMENT, out);
            }
            None => out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid),
        },
        Some(b'-') => {
            out.push(at, at + 1, TokenClass::Operator);
            rest(text, at + 1, COMMENT, out);
        }
        Some(_) => return typed(text, at, out),
    }
    LineState::Clear
}

/// `dword:0000001f`, `hex:01,02`, `hex(2):41,00,...`: the type and its data.
fn typed(text: &[u8], at: usize, out: &mut Out<'_>) -> LineState {
    let colon = text
        .get(at..)
        .and_then(|tail| tail.iter().position(|byte| *byte == b':'))
        .map(|offset| at + offset);
    let kind = colon
        .and_then(|colon| text.get(at..colon))
        .unwrap_or_default();
    let hex = kind == b"hex"
        || kind
            .strip_prefix(b"hex(")
            .and_then(|inner| inner.strip_suffix(b")"))
            .is_some_and(|number| !number.is_empty() && number.iter().all(u8::is_ascii_hexdigit));
    match colon {
        Some(colon) if kind == b"dword" => {
            out.push(at, colon + 1, TokenClass::TypeName);
            let start = colon + 1;
            let digits = text
                .get(start..)
                .unwrap_or_default()
                .iter()
                .take_while(|byte| byte.is_ascii_hexdigit())
                .count();
            let class = if digits == 8 {
                TokenClass::Number
            } else {
                TokenClass::Invalid
            };
            out.push(start, start + digits, class);
            rest(text, start + digits, COMMENT, out);
            LineState::Clear
        }
        Some(colon) if hex => {
            out.push(at, colon + 1, TokenClass::TypeName);
            hex_bytes(text, colon + 1, out)
        }
        _ => {
            out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid);
            LineState::Clear
        }
    }
}

/// Bytes as two hex digits each, split by commas; a `\` at the line's end continues them.
fn hex_bytes(text: &[u8], from: usize, out: &mut Out<'_>) -> LineState {
    let mut at = skip_space(text, from);
    while let Some(&byte) = text.get(at) {
        match byte {
            b',' => {
                out.push(at, at + 1, TokenClass::Punctuation);
                at += 1;
            }
            b'\\' if skip_space(text, at + 1) >= text.len() => {
                out.push(at, at + 1, TokenClass::Punctuation);
                return LineState::Continued;
            }
            b';' => {
                out.push(at, text.len(), TokenClass::Comment);
                return LineState::Clear;
            }
            _ => {
                let length = text
                    .get(at..)
                    .unwrap_or_default()
                    .iter()
                    .take_while(|byte| !matches!(byte, b',' | b' ' | b'\t'))
                    .count()
                    .max(1);
                let item = text.get(at..at + length).unwrap_or_default();
                let class = if item.len() == 2 && item.iter().all(u8::is_ascii_hexdigit) {
                    TokenClass::Number
                } else {
                    TokenClass::Invalid
                };
                out.push(at, at + length, class);
                at += length;
            }
        }
        at = skip_space(text, at);
    }
    LineState::Clear
}

#[cfg(test)]
#[path = "reg_tests.rs"]
mod tests;
