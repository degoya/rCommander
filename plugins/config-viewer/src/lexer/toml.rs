//! TOML: tables `[a.b]` and arrays of tables `[[a]]` (a keyword each), bare, quoted and dotted
//! keys, the four kinds of strings - `"""` and `'''` over lines -, numbers, booleans, dates and
//! times, and arrays and inline tables, which may go on over lines. Where a key may come is
//! known from the state: at a line's start outside any array, and in an inline table after `{`
//! or `,`.

use super::{
    LineState, Multi, Out, TomlState, char_end, closing, is_space, rest, skip_space, toml_values,
    trim_end,
};
use crate::types::TokenClass;

pub(super) fn toml(text: &[u8], state: LineState, out: &mut Out<'_>) -> LineState {
    let mut st = match state {
        LineState::Toml(st) => st,
        _ => TomlState::default(),
    };
    let mut at = 0;
    let mut expect_key = st.depth == 0 || st.in_table();
    if st.string != Multi::None {
        match multi_end(text, 0, st.string) {
            Some(end) => {
                out.push(0, end, TokenClass::String);
                at = end;
                st.string = Multi::None;
                expect_key = false;
            }
            None => {
                out.push(0, text.len(), TokenClass::String);
                return st.end();
            }
        }
    } else if st.depth == 0 {
        let first = skip_space(text, 0);
        if text.get(first) == Some(&b'[') {
            header(text, first, out);
            return LineState::Clear;
        }
    }
    while let Some(&byte) = text.get(at) {
        if is_space(byte) {
            at += 1;
            continue;
        }
        match byte {
            b'#' => {
                out.push(at, text.len(), TokenClass::Comment);
                break;
            }
            b'=' | b'.' if expect_key => {
                out.push(at, at + 1, TokenClass::Punctuation);
                expect_key = byte == b'.';
                at += 1;
            }
            b',' => {
                out.push(at, at + 1, TokenClass::Punctuation);
                expect_key = st.in_table();
                at += 1;
            }
            b'[' | b'{' if !expect_key => {
                out.push(at, at + 1, TokenClass::Punctuation);
                st.open(byte == b'{');
                expect_key = byte == b'{';
                at += 1;
            }
            b']' | b'}' => {
                let class = if st.close(byte == b'}') {
                    TokenClass::Punctuation
                } else {
                    TokenClass::Invalid
                };
                out.push(at, at + 1, class);
                expect_key = false;
                at += 1;
            }
            b'"' | b'\'' => {
                let (kind, opening): (Multi, &[u8]) = if byte == b'"' {
                    (Multi::Basic, b"\"\"\"")
                } else {
                    (Multi::Literal, b"'''")
                };
                let multi =
                    !expect_key && text.get(at..).is_some_and(|tail| tail.starts_with(opening));
                if multi {
                    match multi_end(text, at + 3, kind) {
                        Some(end) => {
                            out.push(at, end, TokenClass::String);
                            at = end;
                        }
                        None => {
                            out.push(at, text.len(), TokenClass::String);
                            st.string = kind;
                            break;
                        }
                    }
                } else {
                    match closing(text, at + 1, byte, byte == b'"') {
                        Some(end) => {
                            let class = if expect_key {
                                TokenClass::Key
                            } else {
                                TokenClass::String
                            };
                            out.push(at, end, class);
                            at = end;
                        }
                        None => {
                            out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid);
                            break;
                        }
                    }
                }
            }
            _ if expect_key => {
                let length = text
                    .get(at..)
                    .unwrap_or_default()
                    .iter()
                    .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                    .count();
                if length == 0 {
                    let end = char_end(text, at);
                    out.push(at, end, TokenClass::Invalid);
                    at = end;
                } else {
                    out.push(at, at + length, TokenClass::Key);
                    at += length;
                }
            }
            _ => {
                let end = value_end(text, at);
                if end == at {
                    let end = char_end(text, at);
                    out.push(at, end, TokenClass::Invalid);
                    at = end;
                } else {
                    let class = toml_values::class(text.get(at..end).unwrap_or_default());
                    out.push(at, end, class);
                    at = end;
                }
            }
        }
    }
    st.end()
}

/// A table header `[a.b]` or an array of tables `[[a]]`, quoted keys inside: one keyword, then
/// at most a comment.
fn header(text: &[u8], at: usize, out: &mut Out<'_>) {
    let double = text.get(at + 1) == Some(&b'[');
    let mut pos = at + if double { 2 } else { 1 };
    while let Some(&byte) = text.get(pos) {
        match byte {
            b'"' | b'\'' => match closing(text, pos + 1, byte, byte == b'"') {
                Some(end) => pos = end,
                None => break,
            },
            b']' => {
                let end = if !double {
                    pos + 1
                } else if text.get(pos + 1) == Some(&b']') {
                    pos + 2
                } else {
                    break;
                };
                out.push(at, end, TokenClass::Keyword);
                rest(text, end, b"#", out);
                return;
            }
            _ => pos += 1,
        }
    }
    out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid);
}

/// After the `"""` or `'''` that closes a multi-line string, searched from `from`; up to two
/// quotes right before it belong to the string. `None` when the line ends first.
fn multi_end(text: &[u8], from: usize, kind: Multi) -> Option<usize> {
    let (quote, escapes) = match kind {
        Multi::Basic => (b'"', true),
        Multi::Literal => (b'\'', false),
        Multi::None => return Some(from),
    };
    let mut at = from;
    while let Some(&byte) = text.get(at) {
        if escapes && byte == b'\\' {
            at += 2;
            continue;
        }
        if byte == quote && text.get(at + 1) == Some(&quote) && text.get(at + 2) == Some(&quote) {
            let mut end = at + 3;
            while end < at + 5 && text.get(end) == Some(&quote) {
                end += 1;
            }
            return Some(end);
        }
        at += 1;
    }
    None
}

/// Where a bare value from `at` ends: a number, a boolean or a date, which may hold one space
/// between its date and its time (`1979-05-27 07:32:00`).
fn value_end(text: &[u8], at: usize) -> usize {
    let run = |from: usize| {
        from + text
            .get(from..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| {
                !is_space(**byte)
                    && !matches!(byte, b',' | b']' | b'}' | b'[' | b'{' | b'#' | b'"' | b'\'')
            })
            .count()
    };
    let end = run(at);
    let date = text.get(at..end).is_some_and(toml_values::is_date);
    if date && text.get(end) == Some(&b' ') && text.get(end + 1).is_some_and(u8::is_ascii_digit) {
        return run(end + 1);
    }
    end
}

#[cfg(test)]
#[path = "toml_tests.rs"]
mod tests;
