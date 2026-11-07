//! Visual Studio solutions (`.sln`): the format line, `#` comments, the blocks `Project(...)` ...
//! `EndProject`, `ProjectSection(...)`, `Global`, `GlobalSection(...)` and their ends, and the
//! `key = value` lines inside them. Every line stands alone.
//!
//! A block's opening names it with a type name - the project's name, the section's - which the
//! tree takes for the node's label (`entry::line_kind`).

use super::{LineState, Out, closing, is_space, scalar_class, skip_space, trim_end};
use crate::types::TokenClass;

/// The first line of every solution.
const FORMAT_LINE: &[u8] = b"Microsoft Visual Studio Solution File";

/// A line of a solution.
pub(super) fn sln(text: &[u8], out: &mut Out<'_>) -> LineState {
    let at = skip_space(text, 0);
    let end = trim_end(text, at, text.len());
    let line = text.get(at..end).unwrap_or_default();
    if line.is_empty() {
        return LineState::Clear;
    }
    if line.starts_with(b"#") {
        out.push(at, end, TokenClass::Comment);
        return LineState::Clear;
    }
    if line.starts_with(FORMAT_LINE) {
        // `..., Format Version 12.00`: the version a number.
        let version = line
            .iter()
            .rposition(|byte| *byte == b' ')
            .map_or(end, |space| at + space + 1);
        out.push(at, trim_end(text, at, version), TokenClass::Keyword);
        out.push(version, end, TokenClass::Number);
        return LineState::Clear;
    }
    let word_end = at
        + line
            .iter()
            .position(|byte| !byte.is_ascii_alphabetic())
            .unwrap_or(line.len());
    match &text[at..word_end] {
        b"Project" | b"ProjectSection" | b"GlobalSection" if text.get(word_end) == Some(&b'(') => {
            out.push(at, word_end, TokenClass::Keyword);
            block(text, word_end, end, out);
        }
        b"EndProject" | b"EndProjectSection" | b"Global" | b"EndGlobalSection" | b"EndGlobal"
            if word_end == end =>
        {
            out.push(at, end, TokenClass::Keyword);
        }
        _ => entry(text, at, end, out),
    }
    LineState::Clear
}

/// After a block's keyword at `open` (`(`): `("{type}")` or `(Name)`, then `= "Name", "path",
/// "{guid}"` or `= preSolution`. The project's name, or the section's, is a type name.
fn block(text: &[u8], open: usize, end: usize, out: &mut Out<'_>) {
    let Some(close) = (open + 1..end).find(|&at| text[at] == b')') else {
        out.push(open, end, TokenClass::Invalid);
        return;
    };
    out.push(open, open + 1, TokenClass::Punctuation);
    let inside = trim_end(text, open + 1, close);
    let quoted = text.get(open + 1) == Some(&b'"');
    // A project's type GUID is quoted; a section's name is bare.
    let class = if quoted {
        value_class(&text[open + 1..inside])
    } else {
        TokenClass::TypeName
    };
    out.push(open + 1, inside, class);
    out.push(close, close + 1, TokenClass::Punctuation);
    let at = skip_space(text, close + 1);
    if text.get(at) != Some(&b'=') {
        return;
    }
    out.push(at, at + 1, TokenClass::Punctuation);
    let mut at = skip_space(text, at + 1);
    let mut first = true;
    while at < end {
        match text[at] {
            b',' => {
                out.push(at, at + 1, TokenClass::Punctuation);
                at += 1;
            }
            b'"' => {
                let stop = closing(text, at + 1, b'"', false).unwrap_or(end);
                // A project's name comes first: the node's label.
                let class = if first && quoted {
                    TokenClass::TypeName
                } else {
                    value_class(&text[at..stop])
                };
                out.push(at, stop, class);
                first = false;
                at = stop;
            }
            _ => {
                let stop = (at..end)
                    .find(|&inside| text[inside] == b',' || is_space(text[inside]))
                    .unwrap_or(end);
                // `preSolution`, `postProject`: when a section is read.
                out.push(at, stop, TokenClass::Literal);
                at = stop;
            }
        }
        at = skip_space(text, at);
    }
}

/// `key = value`: a configuration, a dependency, a property. The key may hold blanks, `|` and
/// dots (`Debug|Any CPU.ActiveCfg`).
fn entry(text: &[u8], at: usize, end: usize, out: &mut Out<'_>) {
    let Some(equals) = (at..end).find(|&inside| text[inside] == b'=') else {
        out.push(at, end, TokenClass::Invalid);
        return;
    };
    out.push(at, trim_end(text, at, equals), TokenClass::Key);
    out.push(equals, equals + 1, TokenClass::Punctuation);
    let value = skip_space(text, equals + 1);
    if value < end {
        out.push(value, end, value_class(&text[value..end]));
    }
}

/// A value's class: a GUID `{...}` (also quoted) a literal, a version (`17.0.31903.59`) a number,
/// else as in INI (`TRUE` a literal, a string).
fn value_class(value: &[u8]) -> TokenClass {
    let bare = value
        .strip_prefix(b"\"")
        .and_then(|inner| inner.strip_suffix(b"\""))
        .unwrap_or(value);
    let guid = bare.len() == 38
        && bare.starts_with(b"{")
        && bare.ends_with(b"}")
        && bare[1..37]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() || *byte == b'-');
    let version = bare.first().is_some_and(u8::is_ascii_digit)
        && bare
            .iter()
            .all(|byte| byte.is_ascii_digit() || *byte == b'.');
    if guid {
        TokenClass::Literal
    } else if version {
        TokenClass::Number
    } else if value.first() == Some(&b'"') {
        TokenClass::String
    } else {
        scalar_class(bare)
    }
}

#[cfg(test)]
#[path = "sln_tests.rs"]
mod tests;
