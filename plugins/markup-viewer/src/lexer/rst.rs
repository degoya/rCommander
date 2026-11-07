//! reStructuredText, a line at a time with the next line beside it: section titles (the line
//! above an underline) and their adornments, transitions and table borders, explicit markup
//! (`.. directive::`, `.. _target:`, `.. [1]`, `.. |name|`, comments) with the indented block after
//! it, field lists, bullets and enumerators, line blocks, doctest lines, paragraphs ending in `::`
//! and the literal block after them, and the inline markup of every line of text
//! ([`super::inline`]).

use super::inline::inline;
use super::{Block, Body, Cursor, Emit, LineState};
use crate::types::TokenClass;

/// The characters an adornment line is made of.
const ADORNMENTS: &[u8] = b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";
/// The directives whose content is not reStructuredText.
const CODE_DIRECTIVES: &[&[u8]] = &[
    b"code",
    b"code-block",
    b"literalinclude",
    b"math",
    b"raw",
    b"sourcecode",
];

/// Lexes one line of reStructuredText that starts in `state`, `next` the line after it; returns
/// the state the next line starts in.
pub(crate) fn line<E: Emit>(
    cur: &mut Cursor<'_, E>,
    next: Option<&[u8]>,
    state: LineState,
) -> LineState {
    let line = cur.line;
    let blank = line.trim_ascii().is_empty();
    let indent = cur.blank_end(0);
    let depth = u16::try_from(indent).unwrap_or(u16::MAX);
    if state.underline && !blank {
        cur.pos = indent;
        cur.push(indent, line.trim_ascii_end().len(), TokenClass::Punctuation);
        return LineState::default();
    }
    match state.block {
        Block::Indented { indent: base, body } if blank || depth > base => {
            if !blank {
                cur.pos = indent;
                body_line(cur, body);
            }
            return LineState {
                block: state.block,
                underline: false,
            };
        }
        Block::Expect { .. } if blank => return state,
        Block::Expect { indent: base } if depth > base => {
            cur.pos = indent;
            cur.rest(TokenClass::String);
            return LineState {
                block: Block::Indented {
                    indent: base,
                    body: Body::Literal,
                },
                underline: false,
            };
        }
        _ => {}
    }
    if blank {
        return LineState::default();
    }
    cur.pos = indent;
    text_line(cur, next, indent, depth)
}

/// A line of an indented block, from its first character.
fn body_line<E: Emit>(cur: &mut Cursor<'_, E>, body: Body) {
    match body {
        Body::Literal => cur.rest(TokenClass::String),
        Body::Comment => cur.rest(TokenClass::Comment),
        Body::Directive | Body::Code if option(cur) => {}
        Body::Directive => inline(cur, cur.end()),
        Body::Code => cur.rest(TokenClass::String),
    }
}

/// `:name: value`, a field or a directive's option, at the cursor: the name a key, the value
/// inline text. False, and nothing taken, when the line is none.
fn option<E: Emit>(cur: &mut Cursor<'_, E>) -> bool {
    let start = cur.pos;
    let line = cur.line;
    if cur.byte(start) != Some(b':') {
        return false;
    }
    let Some(close) = (start + 1..line.len()).find(|&at| {
        line[at] == b':'
            && line
                .get(at + 1)
                .is_none_or(|after| *after == b' ' || *after == b'\t')
    }) else {
        return false;
    };
    let name = &line[start + 1..close];
    if name.is_empty() || name[0] == b' ' || name.contains(&b'`') {
        return false;
    }
    cur.take(start, close + 1, TokenClass::Key);
    inline(cur, cur.end());
    true
}

/// A line of text outside every block, from its first character.
fn text_line<E: Emit>(
    cur: &mut Cursor<'_, E>,
    next: Option<&[u8]>,
    indent: usize,
    depth: u16,
) -> LineState {
    let line = cur.line;
    let trimmed_end = line.trim_ascii_end().len();
    let text = &line[indent..trimmed_end];
    if adornment(text, 3) || table_border(text) {
        cur.take(indent, trimmed_end, TokenClass::Punctuation);
        return LineState::default();
    }
    let chars = text.iter().filter(|byte| (**byte & 0xC0) != 0x80).count();
    // An underline stands at the line's start, at least as long as a short title.
    if next.is_some_and(|next| adornment(next.trim_ascii_end(), chars.min(3))) {
        cur.take(indent, trimmed_end, TokenClass::Keyword);
        return LineState {
            block: Block::Text,
            underline: true,
        };
    }
    if text.starts_with(b"..")
        && text
            .get(2)
            .is_none_or(|byte| *byte == b' ' || *byte == b'\t')
    {
        return explicit(cur, indent, depth);
    }
    if option(cur) {
        return LineState::default();
    }
    if let Some(marker) = list_marker(text) {
        let class = if marker.1 {
            TokenClass::Number
        } else {
            TokenClass::Operator
        };
        cur.take(indent, indent + marker.0, class);
        if text.starts_with(b">>>") {
            // A doctest: Python, not reStructuredText.
            cur.pos = cur.blank_end(cur.pos);
            cur.rest(TokenClass::String);
            return LineState::default();
        }
    }
    // A paragraph ending in `::`: a literal block follows.
    if text.ends_with(b"::") {
        let marker = indent + text.len() - 2;
        inline(cur, marker);
        cur.take(marker, marker + 2, TokenClass::Punctuation);
        return LineState {
            block: Block::Expect { indent: depth },
            underline: false,
        };
    }
    inline(cur, trimmed_end);
    LineState::default()
}

/// Explicit markup at `indent` (`..`): a directive, a target, a footnote or citation, a
/// substitution definition, or a comment; the indented block after it is its own.
fn explicit<E: Emit>(cur: &mut Cursor<'_, E>, indent: usize, depth: u16) -> LineState {
    let line = cur.line;
    let after = cur.blank_end(indent + 2);
    let block = |body| LineState {
        block: Block::Indented {
            indent: depth,
            body,
        },
        underline: false,
    };
    let comment = |cur: &mut Cursor<'_, E>| {
        cur.rest(TokenClass::Comment);
        block(Body::Comment)
    };
    match cur.byte(after) {
        None => comment(cur),
        Some(b'_') => {
            // `.. _name: target`; a name in backquotes may hold a colon.
            let colon = (after + 1..line.len())
                .find(|&at| line[at] == b':' && line.get(at + 1).is_none_or(|next| *next == b' '));
            let Some(colon) = colon else {
                return comment(cur);
            };
            cur.push(indent, indent + 2, TokenClass::Punctuation);
            cur.take(after, colon + 1, TokenClass::Key);
            cur.pos = cur.blank_end(cur.pos);
            cur.rest(TokenClass::TypeName);
            LineState::default()
        }
        Some(b'[') => {
            let Some(close) = (after + 1..line.len()).find(|&at| line[at] == b']') else {
                return comment(cur);
            };
            cur.push(indent, indent + 2, TokenClass::Punctuation);
            cur.take(after, close + 1, TokenClass::Key);
            inline(cur, cur.end());
            block(Body::Directive)
        }
        Some(byte) => {
            let mut start = after;
            let mut substitution = None;
            if byte == b'|'
                && let Some(close) = (after + 1..line.len()).find(|&at| line[at] == b'|')
            {
                substitution = Some(close + 1);
                start = cur.blank_end(close + 1);
            }
            let Some(name_end) = directive(line, start) else {
                return comment(cur);
            };
            cur.push(indent, indent + 2, TokenClass::Punctuation);
            if let Some(end) = substitution {
                cur.push(after, end, TokenClass::Key);
            }
            cur.take(start, name_end + 2, TokenClass::Keyword);
            let name = &line[start..name_end];
            if CODE_DIRECTIVES.contains(&name) {
                // A code directive's argument is its language.
                cur.pos = cur.blank_end(cur.pos);
                cur.rest(TokenClass::TypeName);
                block(Body::Code)
            } else {
                inline(cur, cur.end());
                block(Body::Directive)
            }
        }
    }
}

/// The end of a directive's name at `start`, before its `::`; `None` when no directive is there.
fn directive(line: &[u8], start: usize) -> Option<usize> {
    let rest = line.get(start..)?;
    let found = rest.windows(2).position(|pair| pair == b"::")?;
    let name = &rest[..found];
    let valid = !name.is_empty()
        && name
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"-_+.:".contains(b));
    let after = rest.get(found + 2);
    (valid && after.is_none_or(|byte| *byte == b' ' || *byte == b'\t')).then_some(start + found)
}

/// A bullet (`-`, `*`, `+`, U+2022), an enumerator (`1.`, `#.`, `a)`, `(iv)`), a line block's `|` or
/// a doctest's `>>>` at the start of `text`: its width, and whether it is a number.
fn list_marker(text: &[u8]) -> Option<(usize, bool)> {
    let spaced = |width: usize| text.get(width).is_none_or(|b| *b == b' ' || *b == b'\t');
    match text.first()? {
        b'-' | b'*' | b'+' | b'|' if spaced(1) => return Some((1, false)),
        b'>' if text.starts_with(b">>>") && spaced(3) => return Some((3, false)),
        _ if text.starts_with("\u{2022}".as_bytes()) && spaced(3) => return Some((3, false)),
        _ => {}
    }
    let open = usize::from(text[0] == b'(');
    let label = text[open..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'#')
        .count();
    let numeric = text[open..open + label].iter().all(u8::is_ascii_digit);
    let single = label == 1;
    if label == 0
        || !(numeric
            || single
            || text[open..open + label]
                .iter()
                .all(|b| b"ivxlcdm".contains(b)))
    {
        return None;
    }
    let close = open + label;
    let ok = match text.get(close) {
        Some(b'.') => open == 0,
        Some(b')') => true,
        _ => false,
    };
    (ok && spaced(close + 1)).then_some((close + 1, true))
}

/// `text` is one adornment character repeated, at least `min` times.
fn adornment(text: &[u8], min: usize) -> bool {
    let Some(first) = text.first() else {
        return false;
    };
    text.len() >= min.max(1) && ADORNMENTS.contains(first) && text.iter().all(|byte| byte == first)
}

/// A table's border: `+---+---+`, `=====  =====`.
fn table_border(text: &[u8]) -> bool {
    let rules = text.iter().filter(|b| **b == b'-' || **b == b'=').count();
    rules >= 3
        && (text.first() == Some(&b'+') || text.contains(&b' '))
        && text.iter().all(|b| b"+-= ".contains(b))
}

#[cfg(test)]
#[path = "rst_tests.rs"]
mod tests;
