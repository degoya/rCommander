//! INI and its kin - Windows `.ini` and `.inf`, `.cfg`, `.conf`, `.editorconfig`, `.gitconfig` -
//! and the plain lines of `.gitignore` and `.gitattributes`. Every line stands alone.

use super::{
    LineState, Out, closing, comment_start, is_space, rest, scalar_class, skip_space, trim_end,
};
use crate::types::TokenClass;

/// The marks of a comment, at a line's start or after a space.
const COMMENT: &[u8] = b";#";

/// A line of an INI file: a section, a comment, a key and its value, or a key alone.
pub(super) fn ini(text: &[u8], out: &mut Out<'_>) -> LineState {
    let at = skip_space(text, 0);
    match text.get(at) {
        None => {}
        Some(b';' | b'#') => out.push(at, text.len(), TokenClass::Comment),
        Some(b'[') => header(text, at, COMMENT, out),
        Some(_) => entry(text, at, out),
    }
    LineState::Clear
}

/// `[section]`, also `[remote "origin"]`, `[*.{js,py}]` and `[HKEY_...]`: one keyword up to the
/// first `]` that only a comment follows, else up to the first `]` with the rest `invalid`.
pub(super) fn header(text: &[u8], at: usize, marks: &[u8], out: &mut Out<'_>) {
    let brackets = move || (at + 1..text.len()).filter(move |&end| text.get(end) == Some(&b']'));
    let close = brackets()
        .find(|&end| {
            text.get(skip_space(text, end + 1))
                .is_none_or(|byte| marks.contains(byte))
        })
        .or_else(|| brackets().next());
    match close {
        Some(end) => {
            out.push(at, end + 1, TokenClass::Keyword);
            rest(text, end + 1, marks, out);
        }
        None => out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid),
    }
}

/// `key = value`, `key: value`, or a key alone (`bare` in a `.gitconfig` section).
fn entry(text: &[u8], at: usize, out: &mut Out<'_>) {
    let find = |mark: u8| {
        text.get(at..)
            .and_then(|tail| tail.iter().position(|byte| *byte == mark))
            .map(|offset| at + offset)
    };
    let Some(separator) = find(b'=').or_else(|| find(b':')) else {
        let end = comment_start(text, at, COMMENT).unwrap_or(text.len());
        out.push(at, trim_end(text, at, end), TokenClass::Key);
        out.push(end, text.len(), TokenClass::Comment);
        return;
    };
    out.push(at, trim_end(text, at, separator), TokenClass::Key);
    out.push(separator, separator + 1, TokenClass::Punctuation);
    values(text, separator + 1, out);
}

/// A value: items split by commas (an `.inf` list), each quoted, a number, a literal or a string;
/// a comment after a space ends it.
fn values(text: &[u8], from: usize, out: &mut Out<'_>) {
    let mut at = skip_space(text, from);
    while let Some(&byte) = text.get(at) {
        let after_space = at
            .checked_sub(1)
            .and_then(|before| text.get(before))
            .copied()
            .is_some_and(is_space);
        match byte {
            b',' => {
                out.push(at, at + 1, TokenClass::Punctuation);
                at += 1;
            }
            b'"' => match closing(text, at + 1, b'"', true) {
                Some(end) => {
                    out.push(at, end, TokenClass::String);
                    at = end;
                }
                None => {
                    out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid);
                    return;
                }
            },
            b';' | b'#' if after_space || at == from => {
                out.push(at, text.len(), TokenClass::Comment);
                return;
            }
            _ => {
                let end = bare_end(text, at);
                let trimmed = trim_end(text, at, end);
                out.push(at, trimmed, scalar_class(&text[at..trimmed]));
                at = end;
            }
        }
        at = skip_space(text, at);
    }
}

/// Where a bare item from `from` ends: at a comma, a quote, or a space before a comment.
fn bare_end(text: &[u8], from: usize) -> usize {
    let mut at = from + 1;
    while let Some(&byte) = text.get(at) {
        let comment = is_space(byte) && matches!(text.get(skip_space(text, at)), Some(b';' | b'#'));
        if byte == b',' || byte == b'"' || comment {
            break;
        }
        at += 1;
    }
    at.min(text.len())
}

/// A line of `.gitignore` or `.gitattributes`: a comment, or a pattern (with the `!` that negates
/// it) and the attributes after it (`text`, `-text`, `!eol`, `eol=lf`).
pub(super) fn lines(text: &[u8], out: &mut Out<'_>) -> LineState {
    let mut at = skip_space(text, 0);
    match text.get(at) {
        None => return LineState::Clear,
        Some(b'#') => {
            out.push(at, text.len(), TokenClass::Comment);
            return LineState::Clear;
        }
        Some(_) => {}
    }
    let end = word_end(text, at);
    out.push(at, end, TokenClass::Key);
    at = skip_space(text, end);
    while at < text.len() {
        if matches!(text.get(at), Some(b'-' | b'!')) {
            out.push(at, at + 1, TokenClass::Punctuation);
            at += 1;
        }
        let end = word_end(text, at);
        let word = text.get(at..end).unwrap_or_default();
        match word.iter().position(|byte| *byte == b'=') {
            Some(equals) => {
                out.push(at, at + equals, TokenClass::String);
                out.push(at + equals, at + equals + 1, TokenClass::Punctuation);
                out.push(at + equals + 1, end, TokenClass::String);
            }
            None => out.push(at, end, TokenClass::String),
        }
        at = skip_space(text, end.max(at + 1));
    }
    LineState::Clear
}

/// The end of a word from `at`: the next space that no backslash escapes.
fn word_end(text: &[u8], mut at: usize) -> usize {
    while let Some(&byte) = text.get(at) {
        if is_space(byte) {
            break;
        }
        at += if byte == b'\\' { 2 } else { 1 };
    }
    at.min(text.len())
}

#[cfg(test)]
#[path = "ini_tests.rs"]
mod tests;
