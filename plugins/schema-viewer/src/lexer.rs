//! The schema lexer: one line at a time, in the state the line before left, for both passes (the
//! line index of `open` and the tokens of the lines on screen). It reads the UTF-8 text the Lister
//! shows, so every offset is a byte of it.
//!
//! Protocol Buffers and GraphQL share what this file holds - strings, numbers, the block
//! constructs that run over lines, the role a declaring keyword gives the next name - and each
//! has its own pass over code: [`proto`] (`//` and `/* */` comments, `"..."` and `'...'`
//! strings, dotted type names, `map<K, V>`, `rpc` signatures) and [`graphql`] (`#` comments,
//! `"..."` and block strings `"""..."""`, `$variables`, `@directives`, fragment spreads).
//!
//! What reaches from one line into the next is the [`LineState`]: a block comment (Protobuf) or
//! a block string (GraphQL) still open. A `"..."` string ends with its line in both languages;
//! one its line leaves open shows its opening quote as `invalid`. The lexer colours, it does not
//! parse, and never fails: a file that is not well formed is coloured as far as it reads. It
//! never goes back, and what it looks ahead at is bounded by the line, so the time is linear in
//! the line. Every decision is on ASCII bytes, so every token sits on character boundaries.

mod graphql;
mod proto;
mod words;

use crate::lang::Lang;
use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Open {
    /// Code.
    #[default]
    Code,
    /// Protobuf's `/* ... */`.
    Comment,
    /// GraphQL's block string `""" ... """`.
    BlockString,
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) open: Open,
}

/// What the next name is, after a keyword that declares one. It holds for the line only: both
/// languages declare a name on the keyword's line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Whatever the name itself says.
    #[default]
    Expr,
    /// A type: after `message`, `enum`, `service`, `extend` (Protobuf), `type`, `interface`,
    /// `union`, `input`, `scalar`, `implements`, `on`, `&`, `|` (GraphQL).
    TypeName,
    /// A function: after `rpc` (Protobuf), an operation's or fragment's keyword and a fragment
    /// spread's `...` (GraphQL).
    FunctionName,
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

/// Lexes `line` of a `lang` file (UTF-8, without its line break and a `\r` before it) from
/// `state`, reporting to `emit`. Returns the state the next line starts in.
pub(crate) fn lex_line(
    lang: Lang,
    line: &[u8],
    state: LineState,
    emit: &mut impl Emit,
) -> LineState {
    let mut lexer = Lexer {
        line,
        pos: 0,
        mode: Mode::Expr,
        angle: false,
        signature: false,
        params: false,
        union: false,
        emit,
    };
    let mut open = state.open;
    while lexer.pos < line.len() {
        open = match open {
            Open::Code => match lang {
                Lang::Proto => lexer.proto(),
                Lang::GraphQl => lexer.graphql(),
            },
            Open::Comment => lexer.block_comment(lexer.pos),
            Open::BlockString => lexer.block_string(lexer.pos),
        };
    }
    LineState { open }
}

pub(crate) struct Lexer<'a, E: Emit> {
    line: &'a [u8],
    pos: usize,
    /// What the next name is.
    mode: Mode,
    /// Protobuf: inside `map<...>`, where every name is a type.
    angle: bool,
    /// Protobuf: the line declares an `rpc`, whose brackets hold the message types.
    signature: bool,
    /// Protobuf: inside an `rpc`'s brackets.
    params: bool,
    /// GraphQL: the line declares a `union`, whose `=` lists its member types.
    union: bool,
    emit: &'a mut E,
}

impl<E: Emit> Lexer<'_, E> {
    fn byte(&self, at: usize) -> Option<u8> {
        self.line.get(at).copied()
    }

    fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        self.emit.token(start, end, class);
    }

    /// `width` bytes from `at` are one token of `class`; the next name is whatever it says.
    fn mark(&mut self, at: usize, width: usize, class: TokenClass) {
        self.pos = at + width;
        self.push(at, self.pos, class);
        self.mode = Mode::Expr;
    }

    /// The end of the run from `at` of bytes `keep` takes.
    fn run(&self, at: usize, keep: fn(u8) -> bool) -> usize {
        at + self
            .line
            .get(at..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| keep(**byte))
            .count()
    }

    /// The first byte after `at` that is no blank.
    fn skip_blanks(&self, at: usize) -> usize {
        self.run(at, words::is_blank)
    }

    /// The line from `start` to its end is one token of `class`.
    fn rest_of_line(&mut self, start: usize, class: TokenClass) {
        self.pos = self.line.len();
        self.push(start, self.pos, class);
    }

    /// A block comment from `start`, `pos` inside it: on to its `*/`, or the line's end.
    fn block_comment(&mut self, start: usize) -> Open {
        let rest = self.line.get(self.pos..).unwrap_or_default();
        match rest.windows(2).position(|pair| pair == b"*/") {
            Some(close) => {
                self.pos += close + 2;
                self.push(start, self.pos, TokenClass::Comment);
                Open::Code
            }
            None => {
                self.rest_of_line(start, TokenClass::Comment);
                Open::Comment
            }
        }
    }

    /// A GraphQL block string from `start`, `pos` inside it: on past its closing `"""` - `\"""`
    /// is text - or to the line's end.
    fn block_string(&mut self, start: usize) -> Open {
        while self.pos < self.line.len() {
            let rest = self.line.get(self.pos..).unwrap_or_default();
            if rest.starts_with(b"\\\"\"\"") {
                self.pos += 4;
            } else if rest.starts_with(b"\"\"\"") {
                self.pos += 3;
                self.push(start, self.pos, TokenClass::String);
                return Open::Code;
            } else {
                self.pos += 1;
            }
        }
        self.push(start, self.pos, TokenClass::String);
        Open::BlockString
    }

    /// The string `quote` opens at `start`, with backslash escapes, on its line: a string the line
    /// leaves open shows its opening quote as `invalid`, as both languages refuse it.
    fn quoted(&mut self, start: usize, quote: u8) {
        let mut at = start + 1;
        while let Some(byte) = self.byte(at) {
            if byte == b'\\' {
                at += 2;
            } else if byte == quote {
                self.mark(start, at + 1 - start, TokenClass::String);
                return;
            } else {
                at += 1;
            }
        }
        self.push(start, start + 1, TokenClass::Invalid);
        self.rest_of_line(start + 1, TokenClass::String);
        self.mode = Mode::Expr;
    }

    /// The number at `at`: `42`, `-7` (GraphQL's sign is the number's), `1.5`, `.5`, `6.02e23`,
    /// `1E-3`, `0x1F`.
    fn number(&mut self, at: usize) {
        let digits = at + usize::from(self.byte(at) == Some(b'-'));
        let hex =
            self.byte(digits) == Some(b'0') && matches!(self.byte(digits + 1), Some(b'x' | b'X'));
        let mut end = if hex {
            self.run(digits + 2, |byte| byte.is_ascii_hexdigit())
        } else {
            let whole = self.run(digits, words::is_digit);
            if self.byte(whole) == Some(b'.') && self.byte(whole + 1).is_some_and(words::is_digit) {
                self.run(whole + 1, words::is_digit)
            } else {
                whole
            }
        };
        if !hex && matches!(self.byte(end), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.byte(end + 1), Some(b'+' | b'-')));
            if self.byte(end + 1 + sign).is_some_and(words::is_digit) {
                end = self.run(end + 1 + sign, words::is_digit);
            }
        }
        self.mark(at, end - at, TokenClass::Number);
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
