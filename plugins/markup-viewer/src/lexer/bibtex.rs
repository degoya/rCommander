//! BibTeX: entries `@type{key, field = value, ...}` (or with parentheses), the type a keyword, the
//! citation key a type name, fields keys, values in braces (nested, over lines) or quotes,
//! numbers, abbreviations and `#` joining them; `@string` and `@preamble` without a key,
//! `@comment` a comment, and every text between entries a comment, as BibTeX ignores it.

use super::{Block, Cursor, Emit, Entry, Value};
use crate::types::TokenClass;

/// Lexes one line of BibTeX that starts in `block`; returns the block the next line starts in.
pub(crate) fn line<E: Emit>(cur: &mut Cursor<'_, E>, block: Block) -> Block {
    let mut entry = match block {
        Block::Entry(entry) => Some(entry),
        _ => None,
    };
    while cur.pos < cur.end() {
        entry = match entry {
            None => outside(cur),
            Some(entry) => inside(cur, entry),
        };
    }
    entry.map_or(Block::Text, Block::Entry)
}

/// Between entries: text up to the next `@`, a comment; an entry's type and opening.
fn outside<E: Emit>(cur: &mut Cursor<'_, E>) -> Option<Entry> {
    let start = cur.pos;
    let line = cur.line;
    let Some(at) = line[start..]
        .iter()
        .position(|byte| *byte == b'@')
        .map(|found| start + found)
    else {
        cur.rest(TokenClass::Comment);
        return None;
    };
    if line[start..at]
        .iter()
        .any(|byte| !byte.is_ascii_whitespace())
    {
        cur.push(start, at, TokenClass::Comment);
    }
    let mut end = at + 1;
    while cur
        .byte(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        end += 1;
    }
    cur.take(at, end, TokenClass::Keyword);
    let kind = line[at + 1..end].to_ascii_lowercase();
    if kind == b"comment" {
        cur.rest(TokenClass::Comment);
        return None;
    }
    let opening = cur.blank_end(end);
    let close = match cur.byte(opening) {
        Some(b'{') => b'}',
        Some(b'(') => b')',
        _ => return None,
    };
    cur.take(opening, opening + 1, TokenClass::Punctuation);
    Some(Entry {
        close,
        key: kind != b"string" && kind != b"preamble",
        value: Value::None,
    })
}

/// Inside an entry: one step - a value going on, the key, a field, a value, a separator, or the
/// entry's end. Returns the entry, or `None` once it closed.
fn inside<E: Emit>(cur: &mut Cursor<'_, E>, mut entry: Entry) -> Option<Entry> {
    match entry.value {
        Value::Braced(depth) => {
            entry.value = value(cur, cur.pos, depth, false);
            return Some(entry);
        }
        Value::Quoted(depth) => {
            entry.value = value(cur, cur.pos, depth, true);
            return Some(entry);
        }
        Value::None => {}
    }
    let start = cur.pos;
    let Some(byte) = cur.byte(start) else {
        return Some(entry);
    };
    match byte {
        b' ' | b'\t' => cur.pos = cur.blank_end(start),
        b'%' => cur.rest(TokenClass::Comment),
        _ if byte == entry.close => {
            cur.take(start, start + 1, TokenClass::Punctuation);
            return None;
        }
        b'{' | b'"' => {
            cur.opened(start, 1);
            entry.key = false;
            entry.value = value(cur, start + 1, 0, byte == b'"');
            // The token starts at the opening.
            return Some(entry);
        }
        b',' => {
            entry.key = false;
            cur.take(start, start + 1, TokenClass::Punctuation);
        }
        b'=' | b'#' => cur.take(start, start + 1, TokenClass::Operator),
        b'0'..=b'9' if !entry.key => {
            let mut end = start;
            while cur.byte(end).is_some_and(|b| b.is_ascii_digit()) {
                end += 1;
            }
            cur.take(start, end, TokenClass::Number);
        }
        _ => {
            let mut end = start;
            while cur.byte(end).is_some_and(|b| {
                !b.is_ascii_whitespace()
                    && !matches!(b, b',' | b'=' | b'#' | b'{' | b'}' | b'"' | b'(' | b')')
            }) {
                end += 1;
            }
            let end = end.max(start + 1);
            let class = if entry.key {
                TokenClass::TypeName
            } else if cur.byte(cur.blank_end(end)) == Some(b'=') {
                TokenClass::Key
            } else {
                // An abbreviation: `jan`, or a name `@string` defined.
                TokenClass::Literal
            };
            entry.key = false;
            cur.take(start, end, class);
        }
    }
    Some(entry)
}

/// A value's text from `from` (its opening at the cursor, or the line's start): braces nest, a
/// quoted value ends at a `"` outside them. Returns what is still open at the line's end.
fn value<E: Emit>(cur: &mut Cursor<'_, E>, from: usize, depth: u8, quoted: bool) -> Value {
    let start = cur.pos;
    let mut depth = depth;
    let mut at = from;
    while let Some(byte) = cur.byte(at) {
        match byte {
            b'\\' => at += 1,
            b'{' => depth = depth.saturating_add(1),
            b'}' if depth > 0 => depth -= 1,
            b'}' if !quoted => {
                cur.take(start, at + 1, TokenClass::String);
                cur.closed();
                return Value::None;
            }
            b'"' if quoted && depth == 0 => {
                cur.take(start, at + 1, TokenClass::String);
                cur.closed();
                return Value::None;
            }
            _ => {}
        }
        at += 1;
    }
    cur.rest(TokenClass::String);
    if quoted {
        Value::Quoted(depth)
    } else {
        Value::Braced(depth)
    }
}

#[cfg(test)]
#[path = "bibtex_tests.rs"]
mod tests;
