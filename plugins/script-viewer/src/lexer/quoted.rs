//! Strings between delimiters, shared by Ruby and Perl: quotes, Ruby's `%w[...]` and Perl's
//! `q{...}`, regular expressions with their flags, Perl's two-part `s/a/b/` and `tr/a/b/`, and
//! the interpolations of the strings that have them - Ruby's `#{...}` lexed as code, `#@name`,
//! Perl's `$name`, `${name}` and `@name` as variables. A backslash escapes the byte after it.

use super::scan::{is_name, is_name_start};
use super::{Block, Cursor, Emit, Heredoc, MAX_NEST, Quote, Tag, ends_heredoc, ruby};
use crate::types::TokenClass;

/// The most bytes of a Perl `${name}` looked through for its brace.
const MAX_BRACED_NAME: usize = 64;

/// Whose interpolations a string has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Interp {
    Ruby,
    Perl,
}

/// The closing delimiter of `open`: a bracket's partner, else the byte itself.
pub(crate) fn closing(open: u8) -> u8 {
    match open {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        other => other,
    }
}

/// A string delimited by `open`.
pub(crate) fn quote(open: u8, interp: bool) -> Quote {
    Quote {
        open,
        close: closing(open),
        interp,
        ..Quote::default()
    }
}

/// Reads the string `quote` from the cursor on; its token starts at `from` (its opening, or the
/// line's start). Returns [`Block::Code`] with the cursor after the string when it closes, else
/// the string as it stands at the line's end.
pub(crate) fn string<E: Emit>(
    cur: &mut Cursor<'_, E>,
    mut quote: Quote,
    from: usize,
    who: Interp,
) -> Block {
    let mut seg = from;
    let mut at = cur.pos;
    while let Some(byte) = cur.byte(at) {
        if byte == b'\\' {
            at = (at + 2).min(cur.end);
            continue;
        }
        if quote.interp
            && let Some(after) = interpolation(cur, at, seg, who)
        {
            seg = after;
            at = after;
            continue;
        }
        if quote.open != quote.close && byte == quote.open {
            quote.depth = quote.depth.saturating_add(1);
        } else if byte == quote.close && quote.close != 0 {
            if quote.depth > 0 {
                quote.depth -= 1;
            } else if quote.more > 0 {
                quote.more -= 1;
                at += 1;
                if quote.open != quote.close {
                    // `s{a}{b}`, `s{a}/b/`: the next part has delimiters of its own, after blanks.
                    let next = cur.blank_end(at);
                    match cur.byte(next) {
                        Some(open) if !is_name(open) => {
                            quote.open = open;
                            quote.close = closing(open);
                            at = next + 1;
                        }
                        _ => return close(cur, seg, at),
                    }
                }
                continue;
            } else {
                at += 1;
                if quote.flags {
                    while cur.byte(at).is_some_and(|byte| byte.is_ascii_alphabetic()) {
                        at += 1;
                    }
                }
                return close(cur, seg, at);
            }
        }
        at += 1;
    }
    cur.push(seg, cur.end, TokenClass::String);
    cur.pos = cur.end;
    Block::Str(quote)
}

/// The string ends before `at`: its last piece from `seg`.
fn close<E: Emit>(cur: &mut Cursor<'_, E>, seg: usize, at: usize) -> Block {
    cur.push(seg, at, TokenClass::String);
    cur.pos = at;
    cur.value = true;
    cur.closed();
    Block::Code
}

/// An interpolation at `at`, the string's text from `seg` before it: its tokens, and where the
/// string goes on after it. `None` when nothing is interpolated there.
fn interpolation<E: Emit>(
    cur: &mut Cursor<'_, E>,
    at: usize,
    seg: usize,
    who: Interp,
) -> Option<usize> {
    let end = match who {
        Interp::Ruby => return ruby_interpolation(cur, at, seg),
        Interp::Perl => perl_variable(cur, at)?,
    };
    cur.push(seg, at, TokenClass::String);
    cur.push(at, end, TokenClass::Key);
    Some(end)
}

/// Ruby's `#{code}`, lexed as code up to its brace on the same line, and `#@name`, `#@@name`,
/// `#$name`.
fn ruby_interpolation<E: Emit>(cur: &mut Cursor<'_, E>, at: usize, seg: usize) -> Option<usize> {
    if cur.byte(at) != Some(b'#') {
        return None;
    }
    match cur.byte(at + 1)? {
        b'{' if cur.nest < MAX_NEST => {
            let close = matching_brace(cur, at + 2)?;
            cur.push(seg, at, TokenClass::String);
            cur.push(at, at + 2, TokenClass::Punctuation);
            ruby::code(&mut cur.inner(at + 2, close));
            cur.push(close, close + 1, TokenClass::Punctuation);
            Some(close + 1)
        }
        b'@' | b'$' => {
            let mut name = at + 2;
            if cur.byte(at + 1) == Some(b'@') && cur.byte(name) == Some(b'@') {
                name += 1;
            }
            if !cur.byte(name).is_some_and(is_name_start) {
                return None;
            }
            let end = cur.name_end(name);
            cur.push(seg, at, TokenClass::String);
            cur.push(at, end, TokenClass::Key);
            Some(end)
        }
        _ => None,
    }
}

/// The `}` that closes the brace opened before `from`, on the same line.
fn matching_brace<E: Emit>(cur: &Cursor<'_, E>, from: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = from;
    while let Some(byte) = cur.byte(at) {
        match byte {
            b'\\' => at += 1,
            b'{' => depth += 1,
            b'}' if depth == 0 => return Some(at),
            b'}' => depth -= 1,
            _ => {}
        }
        at += 1;
    }
    None
}

/// The end of the Perl variable at `at` inside a string: `$name`, `$pkg::name`, `${name}`,
/// `@name`, `@{name}`; `None` when `$` or `@` stands for itself (`$` ending a pattern, an `@`
/// before a blank).
pub(crate) fn perl_variable<E: Emit>(cur: &Cursor<'_, E>, at: usize) -> Option<usize> {
    if !matches!(cur.byte(at), Some(b'$' | b'@')) {
        return None;
    }
    let mut name = at + 1;
    if cur.byte(at) == Some(b'$') {
        while cur.byte(name) == Some(b'$') {
            name += 1;
        }
    }
    match cur.byte(name)? {
        b'{' => {
            let close = (name + 1..(name + MAX_BRACED_NAME).min(cur.end))
                .find(|&inside| cur.byte(inside) == Some(b'}'))?;
            Some(close + 1)
        }
        byte if is_name(byte) || byte == b':' => {
            let mut end = name;
            loop {
                if cur.looking_at(end, b"::") && cur.byte(end + 2).is_some_and(is_name_start) {
                    end += 2;
                }
                let next = cur.name_end(end);
                if next == end {
                    break;
                }
                end = next;
            }
            (end > name).then_some(end)
        }
        _ => None,
    }
}

/// The heredoc opened at `start` (`<<`): `<<~TAG` and `<<-TAG` (Ruby only) let the tag stand
/// indented, `<<'TAG'` interpolates nothing, `<<"TAG"` and a bare `<<TAG` do. A bare tag is a
/// name, and in Ruby, without `~` or `-`, one starting with a capital or `_` (`a <<b` pushes).
/// Returns where the opening ends and the heredoc; `None` when there is none, or its tag is
/// longer than [`super::MAX_TAG`].
pub(crate) fn heredoc_at<E: Emit>(
    cur: &Cursor<'_, E>,
    start: usize,
    who: Interp,
) -> Option<(usize, Heredoc)> {
    if !cur.looking_at(start, b"<<") {
        return None;
    }
    let mut at = start + 2;
    let indented = match cur.byte(at) {
        Some(b'~') => true,
        Some(b'-') => who == Interp::Ruby,
        _ => false,
    };
    if indented {
        at += 1;
    }
    let (from, to, end, interp) = match cur.byte(at)? {
        mark @ (b'\'' | b'"' | b'`') => {
            let close = (at + 1..cur.end).find(|&inside| cur.byte(inside) == Some(mark))?;
            (at + 1, close, close + 1, mark != b'\'')
        }
        first
            if is_name_start(first)
                && (indented
                    || who == Interp::Perl
                    || first.is_ascii_uppercase()
                    || first == b'_') =>
        {
            let end = cur.name_end(at);
            (at, end, end, true)
        }
        _ => return None,
    };
    let tag = Tag::new(cur.line.get(from..to)?)?;
    Some((
        end,
        Heredoc {
            tag,
            indented,
            interp,
        },
    ))
}

/// A line of `heredoc`'s body: string text, with its interpolations when it has them; the line
/// that is its tag closes it.
pub(crate) fn heredoc_line<E: Emit>(
    cur: &mut Cursor<'_, E>,
    heredoc: Heredoc,
    who: Interp,
) -> Block {
    if ends_heredoc(cur.line, &heredoc) {
        cur.rest(TokenClass::String);
        cur.closed();
        return Block::Code;
    }
    if heredoc.interp {
        // No delimiter closes a body: only its tag line does.
        let body = Quote {
            interp: true,
            ..Quote::default()
        };
        string(cur, body, 0, who);
    } else {
        cur.rest(TokenClass::String);
    }
    Block::Heredoc(heredoc)
}

#[cfg(test)]
#[path = "quoted_tests.rs"]
mod tests;
