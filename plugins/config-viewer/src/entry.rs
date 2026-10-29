//! What a line is to the tree, told from its tokens: a header (`[section]`, `[a.b]`, `[[a]]`,
//! `[HKEY_...]`), an entry (a key, maybe after `export`, with its value), or neither. A line that
//! starts inside a value of the line before (a continuation, a multi-line string, an array) is
//! neither.

use crate::lexer::LineState;
use crate::tree::preview;
use crate::types::{Token, TokenClass};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LineKind<'t> {
    Other,
    Entry,
    Header { array: bool, name: &'t str },
}

fn piece<'t>(text: &'t str, token: &Token) -> &'t str {
    let start = token.start as usize;
    text.get(start..start + token.len as usize)
        .unwrap_or_default()
}

/// The kind of line `text`, which started in `start` and has `tokens`.
pub(crate) fn line_kind<'t>(text: &'t str, start: LineState, tokens: &[Token]) -> LineKind<'t> {
    if start != LineState::Clear {
        return LineKind::Other;
    }
    let mut tokens = tokens.iter();
    let Some(first) = tokens.next() else {
        return LineKind::Other;
    };
    let word = piece(text, first);
    match first.class {
        TokenClass::Key => LineKind::Entry,
        TokenClass::Keyword if word.starts_with('[') => {
            let array = word.starts_with("[[") && word.ends_with("]]");
            let inner = if array {
                word.get(2..word.len() - 2)
            } else {
                word.strip_prefix('[')
                    .and_then(|inner| inner.strip_suffix(']'))
            };
            LineKind::Header {
                array,
                name: inner.unwrap_or(word).trim(),
            }
        }
        TokenClass::Keyword
            if tokens
                .next()
                .is_some_and(|next| next.class == TokenClass::Key) =>
        {
            LineKind::Entry
        }
        _ => LineKind::Other,
    }
}

/// The label and value of entry line `text`: the key as written (a quoted one without its
/// quotes), and the value up to a comment, cut short; `continues`: the value goes on in the next
/// line.
pub(crate) fn entry(text: &str, tokens: &[Token], continues: bool) -> (String, Option<String>) {
    let Some(first) = tokens
        .iter()
        .position(|token| token.class == TokenClass::Key)
    else {
        return (String::new(), None);
    };
    let span = |token: &Token| (token.start as usize, (token.start + token.len) as usize);
    let (label_start, mut label_end) = tokens.get(first).map(span).unwrap_or_default();
    let mut value_start = label_end;
    let mut value_end = text.len();
    let mut in_key = true;
    for token in tokens.iter().skip(first + 1) {
        let (start, end) = span(token);
        match (token.class, piece(text, token)) {
            // A dotted key: `a.b.c`.
            (TokenClass::Key, _) | (TokenClass::Punctuation, ".") if in_key => {
                label_end = end;
                value_start = end;
            }
            (TokenClass::Punctuation, "=" | ":") if in_key => {
                value_start = end;
                in_key = false;
            }
            (TokenClass::Comment, _) => {
                value_end = start;
                break;
            }
            _ => in_key = false,
        }
    }
    let label = text.get(label_start..label_end).unwrap_or_default().trim();
    let label = ['"', '\'']
        .iter()
        .find_map(|quote| label.strip_prefix(*quote)?.strip_suffix(*quote))
        .unwrap_or(label);
    let value = text
        .get(value_start..value_end.max(value_start))
        .unwrap_or_default()
        .trim();
    let value = (!value.is_empty() || continues).then(|| preview(value, continues));
    (preview(label, false), value)
}

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;
