//! Perl: `#` comments and POD (`=head1` ... `=cut`), strings and the quote-like operators (`q`,
//! `qq`, `qw`, `qx`, `qr`, `m`, `s`, `tr`, `y`) with their delimiters and flags, interpolated
//! variables, regular expressions where a value cannot stand, `$scalar`, `@array`, `%hash`,
//! `$#last`, heredocs, the name after `sub` and the module after `package` and `use`, barewords
//! before `=>`, and `__END__`/`__DATA__` with the POD after them.

use super::quoted::{self, Interp, heredoc_at, heredoc_line, perl_variable, quote};
use super::ruby::starts_with_word;
use super::scan::{is_name, is_name_start};
use super::{Block, Cursor, Emit, Heredoc, Quote};
use crate::lang::Lang;
use crate::types::TokenClass;

/// What the word before declares: the next name is coloured as what it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Declares {
    /// `sub`: a function.
    Sub,
    /// `package`, `use`, `no`, `require`: a module.
    Module,
}

/// Lexes one line of Perl that starts in `block`; returns the block the next line starts in.
pub(crate) fn line<E: Emit>(line: &[u8], block: Block, emit: &mut E) -> Block {
    let mut cur = Cursor::new(line, emit);
    let pod = line.first() == Some(&b'=') && line.get(1).is_some_and(u8::is_ascii_alphabetic);
    let cut = starts_with_word(line, b"=cut");
    match block {
        Block::Doc { data } => {
            cur.rest(TokenClass::Comment);
            return match (cut, data) {
                (false, _) => block,
                (true, false) => Block::Code,
                (true, true) => Block::Data,
            };
        }
        Block::Data if pod && !cut => {
            cur.rest(TokenClass::Comment);
            return Block::Doc { data: true };
        }
        Block::Data => return Block::Data,
        Block::Heredoc(heredoc) => return heredoc_line(&mut cur, heredoc, Interp::Perl),
        Block::Code if pod => {
            cur.rest(TokenClass::Comment);
            return if cut {
                Block::Code
            } else {
                Block::Doc { data: false }
            };
        }
        Block::Code
            if starts_with_word(line, b"__END__") || starts_with_word(line, b"__DATA__") =>
        {
            cur.rest(TokenClass::Keyword);
            return Block::Data;
        }
        _ => {}
    }
    let mut heredoc = None;
    let mut block = block;
    if let Block::Str(open) = block {
        block = quoted::string(&mut cur, open, 0, Interp::Perl);
    }
    if block == Block::Code {
        block = run(&mut cur, &mut heredoc);
    }
    match heredoc {
        Some(heredoc) => {
            if matches!(block, Block::Str(_)) {
                cur.failed();
            }
            Block::Heredoc(heredoc)
        }
        None => block,
    }
}

/// Code up to the line's end, or until a string stays open there; a heredoc opened on the line
/// goes into `heredoc` (the first only).
fn run<E: Emit>(cur: &mut Cursor<'_, E>, heredoc: &mut Option<Heredoc>) -> Block {
    let mut declares = None;
    let mut member = false;
    while let Some(byte) = cur.byte(cur.pos) {
        let start = cur.pos;
        if byte == b' ' || byte == b'\t' {
            cur.pos += 1;
            continue;
        }
        let opened = if is_name_start(byte) {
            let opened = word(cur, &mut declares, member);
            member = false;
            opened
        } else {
            let opened = sigil_or_symbol(cur, heredoc);
            member = cur.line[start..cur.pos].ends_with(b"->");
            declares = None;
            opened
        };
        if let Some((width, open)) = opened {
            cur.opened(start, width);
            cur.pos = start + width;
            let block = quoted::string(cur, open, start, Interp::Perl);
            if block != Block::Code {
                return block;
            }
        }
    }
    Block::Code
}

/// Everything that is not a name: comments, quotes, variables, numbers, heredocs, operators.
fn sigil_or_symbol<E: Emit>(
    cur: &mut Cursor<'_, E>,
    heredoc: &mut Option<Heredoc>,
) -> Option<(usize, Quote)> {
    let start = cur.pos;
    let byte = cur.byte(start)?;
    let next = cur.byte(start + 1);
    match byte {
        b'#' => cur.rest(TokenClass::Comment),
        b'"' | b'`' => return Some((1, quote(byte, true))),
        b'\'' => return Some((1, quote(byte, false))),
        b'/' if !cur.value => {
            return Some((
                1,
                Quote {
                    flags: true,
                    ..quote(b'/', true)
                },
            ));
        }
        b'$' if next == Some(b'#')
            && cur
                .byte(start + 2)
                .is_some_and(|b| b == b'{' || b == b'$' || is_name_start(b)) =>
        {
            let end = cur.name_end(start + 2).max(start + 3);
            key(cur, start, end);
        }
        b'$' | b'@' => match perl_variable(cur, start) {
            Some(end) => key(cur, start, end),
            // `$_` is a name; `$0`, `$!`, `$/`, `$@`, `$$` ... are one byte.
            None if byte == b'$'
                && next.is_some_and(|b| b.is_ascii_punctuation() || b.is_ascii_digit()) =>
            {
                key(cur, start, start + 2);
            }
            None => cur.symbol(),
        },
        b'%' if !cur.value && next.is_some_and(|b| is_name_start(b) || b == b'{' || b == b'$') => {
            match perl_variable(cur, start + 1).or_else(|| Some(cur.name_end(start + 1))) {
                Some(end) if end > start + 1 => key(cur, start, end),
                _ => cur.symbol(),
            }
        }
        b'&' if !cur.value && next.is_some_and(is_name_start) => {
            let end = cur.name_end(start + 1);
            cur.push(start, end, TokenClass::Function);
            cur.pos = end;
            cur.value = true;
        }
        b'0'..=b'9' => cur.number(),
        b'<' => match heredoc_at(cur, start, Interp::Perl) {
            Some((end, found)) => {
                cur.push(start, end, TokenClass::String);
                if heredoc.is_none() && cur.nest == 0 {
                    cur.opened(start, end - start);
                    *heredoc = Some(found);
                }
                cur.pos = end;
                cur.value = true;
            }
            None => readline(cur),
        },
        _ => cur.symbol(),
    }
    None
}

/// `<STDIN>`, `<$fh>`, `<>`: a handle read; else the operator `<`.
fn readline<E: Emit>(cur: &mut Cursor<'_, E>) {
    let start = cur.pos;
    let mut at = start + 1;
    if cur.byte(at) == Some(b'$') {
        at += 1;
    }
    let end = cur.name_end(at);
    if !cur.value && cur.byte(end) == Some(b'>') {
        key(cur, start, end + 1);
    } else {
        cur.symbol();
    }
}

/// A variable (or handle) `start..end`.
fn key<E: Emit>(cur: &mut Cursor<'_, E>, start: usize, end: usize) {
    cur.push(start, end, TokenClass::Key);
    cur.pos = end;
    cur.value = true;
}

/// A name: a keyword, a quote-like operator with its string, a module, a function called or
/// defined, a bareword key, or a plain name.
fn word<E: Emit>(
    cur: &mut Cursor<'_, E>,
    declares: &mut Option<Declares>,
    member: bool,
) -> Option<(usize, Quote)> {
    let start = cur.pos;
    let mut end = cur.name_end(start);
    while cur.looking_at(end, b"::") && cur.byte(end + 2).is_some_and(is_name_start) {
        end = cur.name_end(end + 2);
    }
    let line = cur.line;
    let text = &line[start..end];
    let after = cur.blank_end(end);
    let fat_comma = cur.looking_at(after, b"=>");
    let subscript =
        start > 0 && cur.line.get(start - 1) == Some(&b'{') && cur.byte(end) == Some(b'}');
    if fat_comma || subscript {
        cur.push(start, end, TokenClass::Key);
        cur.pos = end;
        cur.value = true;
        return None;
    }
    let filetest = start > 0 && cur.line.get(start - 1) == Some(&b'-');
    if !member
        && !filetest
        && declares.is_none()
        && let Some(opened) = quote_like(cur, text, end)
    {
        return Some(opened);
    }
    let called = cur.byte(end) == Some(b'(');
    let next_is_arrow = cur.looking_at(after, b"->");
    let class = match declares.take() {
        Some(Declares::Sub) => Some(TokenClass::Function),
        Some(Declares::Module) => Some(TokenClass::TypeName),
        None if member => Some(TokenClass::Function),
        None => match Lang::Perl.classify(text) {
            Some(class) => Some(class),
            None if called => Some(TokenClass::Function),
            None if text.contains(&b':') || (text[0].is_ascii_uppercase() && next_is_arrow) => {
                Some(TokenClass::TypeName)
            }
            None => None,
        },
    };
    if class == Some(TokenClass::Keyword) {
        match text {
            b"sub" => *declares = Some(Declares::Sub),
            b"package" | b"use" | b"no" | b"require" => *declares = Some(Declares::Module),
            _ => {}
        }
    }
    if let Some(class) = class {
        cur.push(start, end, class);
    }
    cur.pos = end;
    // After a keyword or a list operator (`split`, `grep`) a pattern may follow.
    cur.value = !matches!(class, Some(TokenClass::Keyword | TokenClass::Function));
    None
}

/// The quote-like operator `text` ending at `end`, when its delimiter follows: the width of its
/// opening and its string.
fn quote_like<E: Emit>(cur: &Cursor<'_, E>, text: &[u8], end: usize) -> Option<(usize, Quote)> {
    let (interp, more, flags) = match text {
        b"q" | b"qw" => (false, 0, false),
        b"qq" | b"qx" => (true, 0, false),
        b"qr" | b"m" => (true, 0, true),
        b"s" => (true, 1, true),
        b"tr" | b"y" => (false, 1, true),
        _ => return None,
    };
    let at = cur.blank_end(end);
    let delimiter = cur.byte(at)?;
    if is_name(delimiter)
        || delimiter.is_ascii_whitespace()
        || matches!(delimiter, b',' | b';' | b')' | b'=')
        || (at > end && delimiter == b'#')
    {
        return None;
    }
    Some((
        at + 1 - cur.pos,
        Quote {
            more,
            flags,
            ..quote(delimiter, interp && delimiter != b'\'')
        },
    ))
}

#[cfg(test)]
#[path = "perl_tests.rs"]
mod tests;
