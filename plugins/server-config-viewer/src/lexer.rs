//! The server configuration lexer: one line at a time, in the state the line before left, in the
//! grammar of the document's [`Format`], for both passes (the line index of `open` and the tokens
//! of the lines on screen). It reads the UTF-8 text the Lister shows, so every offset is a byte
//! of it.
//!
//! The grammars share their values ([`Lexer::value`]): quoted strings with the variables in
//! them, variables (`$host`, `${name}`, Apache's `%{HTTP_HOST}` and `%1`, OpenSSH's `%h`),
//! numbers with a unit (`10m`, `30s`), addresses (`127.0.0.1:8080`, `10.0.0.0/8`), the literals
//! `on off yes no true false none all any`, and nginx's operators (`=`, `~*`, `^~`). What reaches
//! from one line into the next is the [`LineState`]: in nginx a statement that goes on (its
//! directive was on a line before) and a quoted string left open; in Apache a line continued by
//! a backslash. OpenSSH's files, crontabs and `hosts` have a line per entry.
//!
//! The lexer colours, it does not parse, and never fails. Every decision is on ASCII bytes, so
//! every token sits on character boundaries.

mod blocks;
mod tables;

use crate::format::Format;
use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    /// nginx: the line goes on with a statement begun before; Apache: the line before ended in
    /// a backslash.
    pub(crate) statement: bool,
    /// nginx: the quote of a string the line before left open, or 0.
    pub(crate) quote: u8,
}

/// What the lexer reports besides the state: its tokens.
pub(crate) trait Emit {
    /// The token `start..end` of the line.
    fn token(&mut self, _start: usize, _end: usize, _class: TokenClass) {}
}

/// Only the state.
impl Emit for () {}

/// The tokens of one line, at most [`MAX_TOKENS_PER_LINE`].
impl Emit for Vec<Token> {
    fn token(&mut self, start: usize, end: usize, class: TokenClass) {
        let (Ok(at), Ok(len)) = (
            u32::try_from(start),
            u32::try_from(end.saturating_sub(start)),
        ) else {
            return;
        };
        if len > 0 && self.len() < MAX_TOKENS_PER_LINE {
            self.push(Token {
                start: at,
                len,
                class,
            });
        }
    }
}

/// Blank: a space, a tab, a form feed, a stray `\r`.
pub(crate) fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0B | 0x0C | b'\r')
}

/// The words that are constants, in any case.
const LITERALS: [&[u8]; 9] = [
    b"on", b"off", b"yes", b"no", b"true", b"false", b"none", b"all", b"any",
];

/// The longest unit after a number: `ms`.
const MAX_UNIT: usize = 2;

/// The class of a plain word: a literal, a number, an operator, or none.
fn word_class(word: &[u8]) -> Option<TokenClass> {
    if LITERALS
        .iter()
        .any(|literal| literal.eq_ignore_ascii_case(word))
    {
        return Some(TokenClass::Literal);
    }
    if word.first().is_some_and(u8::is_ascii_digit) {
        let body = word
            .iter()
            .take_while(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b':' | b'/' | b'-'))
            .count();
        let unit = word.len() - body;
        let letters = word[body..].iter().all(u8::is_ascii_alphabetic);
        return (unit <= MAX_UNIT && letters).then_some(TokenClass::Number);
    }
    word.iter()
        .all(|byte| matches!(byte, b'=' | b'~' | b'*' | b'^' | b'!'))
        .then_some(TokenClass::Operator)
}

/// Lexes `line` (UTF-8, without its line break and a `\r` before it) in `format` from `state`,
/// reporting to `emit`. Returns the state the next line starts in.
pub(crate) fn lex_line(
    line: &[u8],
    state: LineState,
    format: Format,
    emit: &mut impl Emit,
) -> LineState {
    let mut lexer = Lexer {
        line,
        pos: 0,
        format,
        emit,
    };
    match format {
        Format::Nginx => lexer.nginx(state),
        Format::Apache => lexer.apache(state),
        Format::Ssh => lexer.ssh(),
        Format::Crontab => lexer.crontab(),
        Format::Hosts => lexer.hosts(),
    }
}

pub(crate) struct Lexer<'a, E: Emit> {
    pub(crate) line: &'a [u8],
    pub(crate) pos: usize,
    format: Format,
    emit: &'a mut E,
}

impl<E: Emit> Lexer<'_, E> {
    pub(crate) fn byte(&self, at: usize) -> Option<u8> {
        self.line.get(at).copied()
    }

    pub(crate) fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        self.emit.token(start, end, class);
    }

    /// The end of the run from `at` of bytes `keep` takes.
    pub(crate) fn run(&self, at: usize, keep: impl Fn(u8) -> bool) -> usize {
        at + self
            .line
            .get(at..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| keep(**byte))
            .count()
    }

    /// Moves `pos` past blanks; the byte there, if the line goes on.
    pub(crate) fn skip_blanks(&mut self) -> Option<u8> {
        self.pos = self.run(self.pos, is_blank);
        self.byte(self.pos)
    }

    /// The comment from `pos` to the line's end.
    pub(crate) fn comment(&mut self) {
        self.push(self.pos, self.line.len(), TokenClass::Comment);
        self.pos = self.line.len();
    }

    /// Whether `byte` ends a word of the document's grammar.
    pub(crate) fn breaks_word(&self, byte: u8) -> bool {
        is_blank(byte)
            || matches!(byte, b'"' | b'\'' | b'$')
            || (byte == b'%' && matches!(self.format, Format::Apache | Format::Ssh))
            || (matches!(byte, b';' | b'{' | b'}' | b'(' | b')') && self.format == Format::Nginx)
            || (byte == b'>' && self.format == Format::Apache)
    }

    /// The variable at `at` (a `$` or `%`): its end, or `None` when there is none.
    fn variable(&self, at: usize) -> Option<usize> {
        let next = self.byte(at + 1)?;
        let braced = |from: usize| {
            self.line
                .get(from..)?
                .iter()
                .position(|byte| *byte == b'}')
                .map(|close| from + close + 1)
        };
        match (self.byte(at)?, next) {
            (b'$', b'{') => braced(at + 2),
            (b'$', _) if next.is_ascii_alphanumeric() || next == b'_' => {
                Some(self.run(at + 1, |byte| byte.is_ascii_alphanumeric() || byte == b'_'))
            }
            (b'%', b'{') if self.format == Format::Apache => braced(at + 2),
            (b'%', _) if self.format == Format::Apache && next.is_ascii_digit() => Some(at + 2),
            (b'%', _) if self.format == Format::Ssh && next.is_ascii_graphic() => Some(at + 2),
            _ => None,
        }
    }

    /// The quoted string from `start`, `pos` inside it, closed by `quote`: its text as `string`,
    /// the variables in it as `function`, a backslash escaping. Whether it closed on this line.
    pub(crate) fn string(&mut self, start: usize, quote: u8) -> bool {
        let mut piece = start;
        while let Some(byte) = self.byte(self.pos) {
            if byte == b'\\' {
                self.pos = (self.pos + 2).min(self.line.len());
            } else if byte == quote {
                self.pos += 1;
                self.push(piece, self.pos, TokenClass::String);
                return true;
            } else if let Some(end) = self.variable(self.pos) {
                self.push(piece, self.pos, TokenClass::String);
                self.push(self.pos, end, TokenClass::Function);
                self.pos = end;
                piece = end;
            } else {
                self.pos += 1;
            }
        }
        self.push(piece, self.pos, TokenClass::String);
        false
    }

    /// One value at `pos`, which is no blank: a quoted string, a variable or a word. The quote
    /// of a string its line leaves open, or 0.
    pub(crate) fn value(&mut self) -> u8 {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            return 0;
        };
        if matches!(byte, b'"' | b'\'') {
            self.pos += 1;
            return if self.string(at, byte) { 0 } else { byte };
        }
        if let Some(end) = self.variable(at) {
            self.push(at, end, TokenClass::Function);
            self.pos = end;
            return 0;
        }
        // A byte that breaks a word and starts nothing (`$` before no name) is a word alone.
        let end = self.run(at, |byte| !self.breaks_word(byte)).max(at + 1);
        if let Some(class) = word_class(&self.line[at..end]) {
            self.push(at, end, class);
        }
        self.pos = end;
        0
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
