//! Ruby: `#` comments and `=begin` ... `=end`, strings in every quote and `%` form with their
//! interpolations, symbols, `@instance`, `@@class` and `$global` variables, regular expressions
//! where a value cannot stand, heredocs, the name after `def`, `class` and `module`, constants,
//! hash keys `name:`, and `__END__`.

use super::quoted::{self, Interp, heredoc_at, heredoc_line, quote};
use super::scan::{is_name, is_name_start};
use super::{Block, Cursor, Emit, Heredoc, Quote};
use crate::lang::Lang;
use crate::types::TokenClass;

/// What the word before declares: the next name is coloured as what it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Declares {
    /// `def`: a method.
    Method,
    /// `class`, `module`: a type.
    Type,
}

/// Lexes one line of Ruby that starts in `block`; returns the block the next line starts in.
pub(crate) fn line<E: Emit>(line: &[u8], block: Block, emit: &mut E) -> Block {
    let mut cur = Cursor::new(line, emit);
    match block {
        Block::Doc { .. } => {
            cur.rest(TokenClass::Comment);
            return if starts_with_word(line, b"=end") {
                Block::Code
            } else {
                block
            };
        }
        Block::Data => return Block::Data,
        Block::Heredoc(heredoc) => return heredoc_line(&mut cur, heredoc, Interp::Ruby),
        Block::Code if starts_with_word(line, b"=begin") => {
            cur.rest(TokenClass::Comment);
            return Block::Doc { data: false };
        }
        Block::Code if line == b"__END__" => {
            cur.rest(TokenClass::Keyword);
            return Block::Data;
        }
        _ => {}
    }
    let mut heredoc = None;
    let mut block = block;
    if let Block::Str(open) = block {
        block = quoted::string(&mut cur, open, 0, Interp::Ruby);
    }
    if block == Block::Code {
        block = run(&mut cur, &mut heredoc);
    }
    match heredoc {
        Some(heredoc) => {
            // A string left open beside a heredoc's opening ends here: the body comes first.
            if matches!(block, Block::Str(_)) {
                cur.failed();
            }
            Block::Heredoc(heredoc)
        }
        None => block,
    }
}

/// Code from the cursor to its end: an interpolation's inside. What it leaves open ends there.
pub(crate) fn code<E: Emit>(cur: &mut Cursor<'_, E>) {
    run(cur, &mut None);
}

/// `line` starts with `word`, alone or before a blank.
pub(crate) fn starts_with_word(line: &[u8], word: &[u8]) -> bool {
    line.starts_with(word)
        && line
            .get(word.len())
            .is_none_or(|byte| byte.is_ascii_whitespace())
}

/// Code up to the cursor's end, or until a string stays open there; a heredoc opened on the line
/// goes into `heredoc` (the first only: its body follows the line).
fn run<E: Emit>(cur: &mut Cursor<'_, E>, heredoc: &mut Option<Heredoc>) -> Block {
    let mut declares = None;
    let mut member = false;
    while let Some(byte) = cur.byte(cur.pos) {
        let start = cur.pos;
        if byte == b' ' || byte == b'\t' {
            cur.pos += 1;
            continue;
        }
        if is_name_start(byte) {
            word(cur, &mut declares, member);
            member = false;
            continue;
        }
        let opened = match byte {
            b'#' => {
                cur.rest(TokenClass::Comment);
                None
            }
            b'"' | b'`' => Some((1, quote(byte, true))),
            b'\'' => Some((1, quote(byte, false))),
            b'/' if !cur.value => Some((
                1,
                Quote {
                    flags: true,
                    ..quote(b'/', true)
                },
            )),
            b'%' => percent(cur).or_else(|| {
                cur.symbol();
                None
            }),
            b':' => colon(cur),
            b'@' | b'$' => {
                variable(cur);
                None
            }
            b'0'..=b'9' => {
                cur.number();
                None
            }
            b'<' => {
                match heredoc_at(cur, start, Interp::Ruby) {
                    Some((end, found)) => {
                        cur.push(start, end, TokenClass::String);
                        if heredoc.is_none() && cur.nest == 0 {
                            cur.opened(start, end - start);
                            *heredoc = Some(found);
                        }
                        cur.pos = end;
                        cur.value = true;
                    }
                    None => cur.symbol(),
                }
                None
            }
            _ => {
                cur.symbol();
                None
            }
        };
        // `def self.name`: the receiver's dot keeps the method's name coming.
        let dot = cur.pos > start && cur.line.get(cur.pos - 1) == Some(&b'.');
        member = dot && !cur.line[start..cur.pos].ends_with(b"..");
        if !(member && declares == Some(Declares::Method)) {
            declares = None;
        }
        if let Some((width, open)) = opened {
            cur.opened(start, width);
            cur.pos = start + width;
            let block = quoted::string(cur, open, start, Interp::Ruby);
            if block != Block::Code {
                return block;
            }
        }
    }
    Block::Code
}

/// A name: a keyword, a constant, a method called or defined, a hash key, or a plain name.
fn word<E: Emit>(cur: &mut Cursor<'_, E>, declares: &mut Option<Declares>, member: bool) {
    let start = cur.pos;
    let mut end = cur.name_end(start);
    // `empty?`, `save!` - but not `a!=b` or `a?b:c` without blanks.
    if matches!(cur.byte(end), Some(b'?' | b'!'))
        && !matches!(cur.byte(end + 1), Some(b'=' | b':'))
        && !cur.byte(end + 1).is_some_and(is_name)
    {
        end += 1;
    }
    let line = cur.line;
    let text = &line[start..end];
    let next = cur.byte(end);
    // A hash key or keyword argument: `name: value`.
    if next == Some(b':')
        && !member
        && cur
            .byte(end + 1)
            .is_none_or(|byte| matches!(byte, b' ' | b'\t' | b',' | b')'))
        && declares.is_none()
    {
        cur.push(start, end + 1, TokenClass::Key);
        cur.pos = end + 1;
        cur.value = false;
        return;
    }
    let called = cur.byte(end) == Some(b'(');
    let class = match *declares {
        // `def self.name`: `self` is the receiver, the name follows its dot.
        Some(Declares::Method) if next == Some(b'.') => Lang::Ruby.classify(text),
        Some(Declares::Method) => {
            *declares = None;
            Some(TokenClass::Function)
        }
        Some(Declares::Type) => {
            *declares = None;
            Some(TokenClass::TypeName)
        }
        None if member => called.then_some(TokenClass::Function),
        None => match Lang::Ruby.classify(text) {
            Some(class) => Some(class),
            None if text[0].is_ascii_uppercase() && !called => Some(TokenClass::TypeName),
            None => called.then_some(TokenClass::Function),
        },
    };
    if class == Some(TokenClass::Keyword) && !member {
        match text {
            b"def" => *declares = Some(Declares::Method),
            b"class" | b"module" => *declares = Some(Declares::Type),
            _ => {}
        }
    }
    if let Some(class) = class {
        cur.push(start, end, class);
    }
    cur.pos = end;
    cur.value = class != Some(TokenClass::Keyword)
        || matches!(
            text,
            b"end" | b"self" | b"super" | b"__FILE__" | b"__LINE__"
        );
}

/// `%w[...]`, `%q(...)`, `%Q{...}`, `%r{...}`, `%(...)`: the width of the opening and its string.
/// `None` when `%` is the operator.
fn percent<E: Emit>(cur: &Cursor<'_, E>) -> Option<(usize, Quote)> {
    let next = cur.byte(cur.pos + 1)?;
    if matches!(
        next,
        b'q' | b'Q' | b'w' | b'W' | b'i' | b'I' | b'r' | b's' | b'x'
    ) {
        let delimiter = cur.byte(cur.pos + 2)?;
        if delimiter.is_ascii_alphanumeric() || delimiter.is_ascii_whitespace() || delimiter == b'_'
        {
            return None;
        }
        let interp = matches!(next, b'Q' | b'W' | b'I' | b'r' | b'x');
        return Some((
            3,
            Quote {
                flags: next == b'r',
                ..quote(delimiter, interp)
            },
        ));
    }
    let bare = matches!(next, b'(' | b'[' | b'{' | b'<' | b'|' | b'!' | b'/' | b'^');
    (bare && !cur.value).then(|| (2, quote(next, true)))
}

/// `::`, a symbol `:name`, `:"..."`, or the operator `:`.
fn colon<E: Emit>(cur: &mut Cursor<'_, E>) -> Option<(usize, Quote)> {
    let start = cur.pos;
    match cur.byte(start + 1) {
        Some(b':') => {
            cur.push(start, start + 2, TokenClass::Operator);
            cur.pos = start + 2;
            cur.value = false;
            None
        }
        Some(b'"') => Some((2, quote(b'"', true))),
        Some(b'\'') => Some((2, quote(b'\'', false))),
        Some(first) if is_name_start(first) => {
            let mut end = cur.name_end(start + 1);
            if matches!(cur.byte(end), Some(b'?' | b'!' | b'=')) && cur.byte(end + 1) != Some(b'=')
            {
                end += 1;
            }
            cur.push(start, end, TokenClass::Literal);
            cur.pos = end;
            cur.value = true;
            None
        }
        _ => {
            cur.symbol();
            None
        }
    }
}

/// `@name`, `@@name`, `$name` and the special globals `$0`, `$!`, `$:` ...; a lone `@` or `$` is an
/// operator.
fn variable<E: Emit>(cur: &mut Cursor<'_, E>) {
    let start = cur.pos;
    let mut name = start + 1;
    if cur.byte(start) == Some(b'@') && cur.byte(name) == Some(b'@') {
        name += 1;
    }
    let end = match cur.byte(name) {
        Some(first) if is_name_start(first) => cur.name_end(name),
        Some(first)
            if cur.byte(start) == Some(b'$')
                && (first.is_ascii_digit() || b"!@&~_/\\;,.<>*$?:\"'`+=".contains(&first)) =>
        {
            name + 1
        }
        _ => {
            cur.symbol();
            return;
        }
    };
    cur.push(start, end, TokenClass::Key);
    cur.pos = end;
    cur.value = true;
}

#[cfg(test)]
#[path = "ruby_tests.rs"]
mod tests;
