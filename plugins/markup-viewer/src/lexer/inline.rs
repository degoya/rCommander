//! reStructuredText's inline markup, within one line: `**strong**`, `*emphasis*`,
//! ``` ``literal`` ```, `` `interpreted text` `` with its role (`:ref:`...``), hyperlink
//! references (`` `text <url>`_ ``, `name_`, `name__`), substitutions `|name|`, footnote and
//! citation references `[1]_`, and standalone addresses (`https://...`).
//!
//! The start-string stands at the line's start or after a blank or one of `' " ( [ { < - / :`,
//! with no blank after it; the end-string after no blank, before a blank, punctuation or the
//! line's end, within [`MAX_INLINE`] bytes. Markup the line does not close stays text: inline
//! markup over lines is not followed.

use super::{Cursor, Emit, MAX_INLINE};
use crate::types::TokenClass;

/// What may stand before a start-string.
const BEFORE_START: &[u8] = b"'\"([{<-/:";
/// What may stand after an end-string.
const AFTER_END: &[u8] = b"'\")]}>-/:.,;!?\\_";
/// Where standalone addresses start.
const SCHEMES: &[&[u8]] = &[b"https://", b"http://", b"ftp://", b"mailto:"];

/// Lexes the inline markup of the cursor's line from its position up to `end`.
pub(crate) fn inline<E: Emit>(cur: &mut Cursor<'_, E>, end: usize) {
    let line = cur.line;
    let mut at = cur.pos;
    while at < end {
        let byte = line[at];
        let start_ok = at == 0 || is_blank(line[at - 1]) || BEFORE_START.contains(&line[at - 1]);
        let next = if start_ok { markup(cur, at, end) } else { None };
        at = match next {
            Some(after) => after,
            None if is_word(byte) => word(cur, at, end, start_ok),
            None => at + 1,
        };
    }
    cur.pos = end.max(cur.pos);
}

/// The markup starting at `at`, if any: its tokens, and where the text goes on.
fn markup<E: Emit>(cur: &mut Cursor<'_, E>, at: usize, end: usize) -> Option<usize> {
    let line = cur.line;
    if SCHEMES
        .iter()
        .any(|scheme| line[at..end].starts_with(scheme))
    {
        let mut stop = at;
        while stop < end && !is_blank(line[stop]) && line[stop] != b'>' {
            stop += 1;
        }
        while stop > at && b".,;:!?)'\"".contains(&line[stop - 1]) {
            stop -= 1;
        }
        cur.push(at, stop, TokenClass::TypeName);
        return Some(stop);
    }
    match line[at] {
        b'*' if line.get(at + 1) == Some(&b'*') => {
            let close = end_string(line, at + 2, end, b"**")?;
            cur.push(at, close + 2, TokenClass::Keyword);
            Some(close + 2)
        }
        b'*' => {
            let close = end_string(line, at + 1, end, b"*")?;
            cur.push(at, close + 1, TokenClass::Literal);
            Some(close + 1)
        }
        b'`' if line.get(at + 1) == Some(&b'`') => {
            let close = end_string(line, at + 2, end, b"``")?;
            cur.push(at, close + 2, TokenClass::String);
            Some(close + 2)
        }
        b'`' => interpreted(cur, at, end),
        b':' => {
            // `:role:` before interpreted text; a domain's role holds colons (`:py:func:`).
            let name_end = (at + 1..end.min(at + MAX_INLINE))
                .find(|&inside| line[inside] == b':' && line.get(inside + 1) == Some(&b'`'))?;
            let name = &line[at + 1..name_end];
            let valid = !name.is_empty()
                && name
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.+:".contains(b));
            if !valid {
                return None;
            }
            let close = end_string(line, name_end + 2, end, b"`")?;
            cur.push(at, name_end + 1, TokenClass::Function);
            cur.push(name_end + 1, close + 1, TokenClass::String);
            Some(close + 1)
        }
        b'|' => {
            let close = end_string(line, at + 1, end, b"|")?;
            let after = suffix(line, close + 1, end);
            cur.push(at, after, TokenClass::Key);
            Some(after)
        }
        b'[' => {
            let close = (at + 1..end.min(at + MAX_INLINE)).find(|&inside| line[inside] == b']')?;
            let label = &line[at + 1..close];
            if label.is_empty()
                || label.iter().any(|b| is_blank(*b))
                || line.get(close + 1) != Some(&b'_')
            {
                return None;
            }
            cur.push(at, close + 2, TokenClass::Key);
            Some(close + 2)
        }
        _ => None,
    }
}

/// `` `text` ``: a hyperlink reference with `_` or `__` after it, interpreted text with a role
/// after it, or interpreted text of the default role.
fn interpreted<E: Emit>(cur: &mut Cursor<'_, E>, at: usize, end: usize) -> Option<usize> {
    let line = cur.line;
    let close = end_string(line, at + 1, end, b"`")?;
    let after = suffix(line, close + 1, end);
    if after > close + 1 {
        cur.push(at, after, TokenClass::TypeName);
        return Some(after);
    }
    cur.push(at, close + 1, TokenClass::String);
    // A role after the text: `text`:role:.
    if line.get(close + 1) == Some(&b':')
        && let Some(role_end) =
            (close + 2..end.min(close + 2 + MAX_INLINE)).find(|&inside| line[inside] == b':')
        && role_end > close + 2
    {
        cur.push(close + 1, role_end + 1, TokenClass::Function);
        return Some(role_end + 1);
    }
    Some(close + 1)
}

/// A word, and a reference `name_` or `name__` when it ends in one.
fn word<E: Emit>(cur: &mut Cursor<'_, E>, at: usize, end: usize, start_ok: bool) -> usize {
    let line = cur.line;
    let mut stop = at;
    // Hyphens, dots and pluses join words: `rc-plugin.name_`.
    while stop < end
        && (is_word(line[stop])
            || (b"-.+".contains(&line[stop]) && stop + 1 < end && is_word(line[stop + 1])))
    {
        stop += 1;
    }
    let name_end = line[at..stop]
        .iter()
        .rposition(|byte| *byte != b'_')
        .map_or(at, |last| at + last + 1);
    let underscores = stop - name_end;
    let ends = stop == end || !is_word(line[stop]);
    if start_ok && name_end > at && (underscores == 1 || underscores == 2) && ends {
        cur.push(at, stop, TokenClass::TypeName);
    }
    stop.max(at + 1)
}

/// The end-string `mark` after `from` (at least one byte of content), within the line up to
/// `end`: after no blank, before a blank, punctuation or the end.
fn end_string(line: &[u8], from: usize, end: usize, mark: &[u8]) -> Option<usize> {
    if from >= end || is_blank(line[from]) {
        return None;
    }
    let limit = end.min(from + MAX_INLINE);
    (from + 1..limit).find(|&at| {
        line[at..end].starts_with(mark)
            && !is_blank(line[at - 1])
            && line.get(at + mark.len()).is_none_or(|after| {
                at + mark.len() >= end || is_blank(*after) || AFTER_END.contains(after)
            })
    })
}

/// After a reference's closing at `at`: past its `_` or `__`, or `at`.
fn suffix(line: &[u8], at: usize, end: usize) -> usize {
    if at < end && line[at] == b'_' {
        if at + 1 < end && line[at + 1] == b'_' {
            at + 2
        } else {
            at + 1
        }
    } else {
        at
    }
}

fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

/// A byte of a word: a letter, a digit, `_`, or any byte of a character beyond ASCII.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

#[cfg(test)]
#[path = "inline_tests.rs"]
mod tests;
