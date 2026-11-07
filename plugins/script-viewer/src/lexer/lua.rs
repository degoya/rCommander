//! Lua: `--` comments and long comments `--[==[ ... ]==]`, long strings `[[ ... ]]`, short strings
//! with escapes (a backslash or `\z` at the line's end carries one on), numbers with hexadecimal
//! fractions and exponents, `...`, labels `::name::`, and the name after `function`, its
//! `a.b:c` parts included.

use super::quoted::{self, Interp, quote};
use super::scan::is_name_start;
use super::{Block, Cursor, Emit};
use crate::lang::Lang;
use crate::types::TokenClass;

/// The most `=` of a long bracket kept; a longer one is no long bracket.
const MAX_LEVEL: usize = 255;

/// Lexes one line of Lua that starts in `block`; returns the block the next line starts in.
pub(crate) fn line<E: Emit>(line: &[u8], block: Block, emit: &mut E) -> Block {
    let mut cur = Cursor::new(line, emit);
    let block = match block {
        Block::Long { level, comment } => long_close(&mut cur, 0, usize::from(level), comment),
        Block::Str(open) => {
            let block = quoted::string(&mut cur, open, 0, Interp::Ruby);
            short_string(&mut cur, block)
        }
        _ => Block::Code,
    };
    if block != Block::Code {
        return block;
    }
    if line.starts_with(b"#!") {
        cur.rest(TokenClass::Comment);
        return Block::Code;
    }
    run(&mut cur)
}

/// Code up to the line's end, or until a string or long bracket stays open there.
fn run<E: Emit>(cur: &mut Cursor<'_, E>) -> Block {
    let mut function = false;
    let mut member = false;
    while let Some(byte) = cur.byte(cur.pos) {
        let start = cur.pos;
        if byte == b' ' || byte == b'\t' {
            cur.pos += 1;
            continue;
        }
        if is_name_start(byte) {
            word(cur, &mut function, member);
            member = false;
            continue;
        }
        let block = match byte {
            b'-' if cur.byte(start + 1) == Some(b'-') => match long_level(cur, start + 2) {
                Some(level) => {
                    cur.pos = start + 2 + level + 2;
                    long_close(cur, start, level, true)
                }
                None => {
                    cur.rest(TokenClass::Comment);
                    Block::Code
                }
            },
            b'[' => match long_level(cur, start) {
                Some(level) => {
                    cur.opened(start, level + 2);
                    cur.pos = start + level + 2;
                    long_close(cur, start, level, false)
                }
                None => {
                    cur.symbol();
                    Block::Code
                }
            },
            b'"' | b'\'' => {
                cur.opened(start, 1);
                cur.pos = start + 1;
                let block = quoted::string(cur, quote(byte, false), start, Interp::Ruby);
                short_string(cur, block)
            }
            b'0'..=b'9' => {
                cur.number();
                Block::Code
            }
            b'.' if cur.byte(start + 1).is_some_and(|b| b.is_ascii_digit()) => {
                cur.number();
                Block::Code
            }
            b':' if cur.byte(start + 1) == Some(b':') => {
                label(cur);
                Block::Code
            }
            // The length operator; nothing else in Lua starts with `#`.
            b'#' => {
                cur.push(start, start + 1, TokenClass::Operator);
                cur.pos += 1;
                cur.value = false;
                Block::Code
            }
            _ => {
                cur.symbol();
                Block::Code
            }
        };
        if block != Block::Code {
            return block;
        }
        // `a.b:c`: a field's or method's name follows; after `function` it is still the name.
        let separator = matches!(&cur.line[start..cur.pos], b"." | b":");
        member = separator;
        function &= separator;
    }
    Block::Code
}

/// A name: a keyword, a constant, a function called or defined, or a plain name.
fn word<E: Emit>(cur: &mut Cursor<'_, E>, function: &mut bool, member: bool) {
    let start = cur.pos;
    let end = cur.name_end(start);
    let line = cur.line;
    let text = &line[start..end];
    let called = matches!(cur.byte(end), Some(b'(' | b'"' | b'\''));
    let naming = *function;
    let class = if naming {
        Some(TokenClass::Function)
    } else if member {
        called.then_some(TokenClass::Function)
    } else {
        match Lang::Lua.classify(text) {
            Some(class) => Some(class),
            None => called.then_some(TokenClass::Function),
        }
    };
    *function = naming || (text == b"function" && class == Some(TokenClass::Keyword));
    if let Some(class) = class {
        cur.push(start, end, class);
    }
    cur.pos = end;
    cur.value = class != Some(TokenClass::Keyword);
}

/// The level of the long bracket opening at `at` (`[[` 0, `[==[` 2); `None` when there is none.
fn long_level<E: Emit>(cur: &Cursor<'_, E>, at: usize) -> Option<usize> {
    if cur.byte(at) != Some(b'[') {
        return None;
    }
    let mut level = 0;
    while cur.byte(at + 1 + level) == Some(b'=') && level < MAX_LEVEL {
        level += 1;
    }
    (cur.byte(at + 1 + level) == Some(b'[')).then_some(level)
}

/// A long bracket of `level` from the cursor, its token from `from`: up to its closing `]==]`, or
/// to the line's end.
fn long_close<E: Emit>(cur: &mut Cursor<'_, E>, from: usize, level: usize, comment: bool) -> Block {
    let class = if comment {
        TokenClass::Comment
    } else {
        TokenClass::String
    };
    let mut at = cur.pos;
    while let Some(byte) = cur.byte(at) {
        if byte == b']'
            && (1..=level).all(|step| cur.byte(at + step) == Some(b'='))
            && cur.byte(at + level + 1) == Some(b']')
        {
            let end = at + level + 2;
            cur.push(from, end, class);
            cur.pos = end;
            if !comment {
                cur.value = true;
                cur.closed();
            }
            return Block::Code;
        }
        at += 1;
    }
    cur.push(from, cur.end, class);
    cur.pos = cur.end;
    Block::Long {
        // At most MAX_LEVEL, which a byte holds.
        level: u8::try_from(level).unwrap_or(u8::MAX),
        comment,
    }
}

/// A short string that `block` leaves open at the line's end goes on only after a backslash or
/// `\z`; else the line ends it, unclosed.
fn short_string<E: Emit>(cur: &mut Cursor<'_, E>, block: Block) -> Block {
    if block == Block::Code || continues(cur.line) {
        return block;
    }
    cur.failed();
    Block::Code
}

/// The line ends in an escaped line break: an odd run of backslashes, or `\z` and blanks.
fn continues(line: &[u8]) -> bool {
    let odd = |text: &[u8]| text.iter().rev().take_while(|byte| **byte == b'\\').count() % 2 == 1;
    let trimmed = line.trim_ascii_end();
    odd(line) || trimmed.strip_suffix(b"z").is_some_and(odd)
}

/// `::name::`, a label; else the operator.
fn label<E: Emit>(cur: &mut Cursor<'_, E>) {
    let start = cur.pos;
    let end = cur.name_end(start + 2);
    if end > start + 2 && cur.looking_at(end, b"::") {
        cur.push(start, end + 2, TokenClass::Key);
        cur.pos = end + 2;
        cur.value = false;
    } else {
        cur.symbol();
    }
}

#[cfg(test)]
#[path = "lua_tests.rs"]
mod tests;
