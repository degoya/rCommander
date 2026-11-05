//! The SQL lexer: one line at a time, in the state the line before left, for both passes (the
//! line index of `open` and the tokens of the lines on screen). It reads the UTF-8 text the Lister
//! shows, so every offset is a byte of it.
//!
//! What reaches from one line into the next is the [`LineState`]: a block comment (nested, as
//! PostgreSQL nests them), a string in `'...'` or an identifier in `"..."` - SQL lets both run over
//! lines - or a PostgreSQL dollar-quoted body (`$$ ... $$`, `$body$ ... $body$`). Identifiers in
//! `` `...` `` and `[...]` end with their line.
//!
//! The lexer colours, it does not parse, and never fails: a script that is not well formed is
//! coloured as far as it reads. There is no dialect parser: one table of keywords serves ANSI,
//! PostgreSQL, MySQL, SQLite and T-SQL ([`words`]). Only MySQL changes how text is read - `#`
//! starts a comment, `"` a string, and a backslash escapes in a string - and only in a file the
//! viewer takes for MySQL's ([`Dialect`]). Every decision is on ASCII bytes, so every token sits
//! on character boundaries.

mod code;
mod words;

use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The longest tag of a dollar-quoted string kept across lines; a longer `$tag$` is no quote.
pub(crate) const MAX_TAG: usize = 30;

/// How the file's text is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// ANSI, PostgreSQL, SQLite, T-SQL: `"..."` names, `''` the only escape in a string.
    #[default]
    Standard,
    /// MySQL and MariaDB: `#` comments, `"..."` strings, backslash escapes.
    MySql,
}

/// The tag of a dollar-quoted string, `$tag$` without its dollars; empty for `$$`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tag {
    len: u8,
    bytes: [u8; MAX_TAG],
}

impl Tag {
    /// The tag `bytes`, when it fits.
    pub(crate) fn new(bytes: &[u8]) -> Option<Self> {
        let mut tag = Self {
            len: u8::try_from(bytes.len()).ok()?,
            bytes: [0; MAX_TAG],
        };
        tag.bytes.get_mut(..bytes.len())?.copy_from_slice(bytes);
        Some(tag)
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or_default()
    }
}

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Open {
    /// Code.
    #[default]
    Code,
    /// A block comment, this many deep.
    Comment(u8),
    /// A string or an identifier closed by `quote`; `backslash`: a backslash escapes.
    Quote { quote: u8, backslash: bool },
    /// A dollar-quoted string.
    Dollar(Tag),
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) open: Open,
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

/// Lexes `line` (UTF-8, without its line break and a `\r` before it) from `state`, reporting to
/// `emit`. Returns the state the next line starts in.
pub(crate) fn lex_line(
    line: &[u8],
    state: LineState,
    dialect: Dialect,
    emit: &mut impl Emit,
) -> LineState {
    let mut lexer = Lexer {
        line,
        pos: 0,
        dialect,
        emit,
    };
    let mut open = state.open;
    while lexer.pos < line.len() {
        open = match open {
            Open::Code => lexer.code(),
            Open::Comment(depth) => lexer.block_comment(0, depth),
            Open::Quote { quote, backslash } => lexer.quoted(0, quote, backslash),
            Open::Dollar(tag) => lexer.dollar(0, tag),
        };
    }
    LineState { open }
}

pub(crate) struct Lexer<'a, E: Emit> {
    pub(crate) line: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) dialect: Dialect,
    emit: &'a mut E,
}

impl<E: Emit> Lexer<'_, E> {
    pub(crate) fn byte(&self, at: usize) -> Option<u8> {
        self.line.get(at).copied()
    }

    pub(crate) fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        self.emit.token(start, end, class);
    }

    /// A block comment from `start`, `depth` deep at `pos`: on to its `*/`, or the line's end.
    pub(crate) fn block_comment(&mut self, start: usize, mut depth: u8) -> Open {
        while self.pos < self.line.len() {
            match (self.byte(self.pos), self.byte(self.pos + 1)) {
                (Some(b'*'), Some(b'/')) => {
                    self.pos += 2;
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.push(start, self.pos, TokenClass::Comment);
                        return Open::Code;
                    }
                }
                (Some(b'/'), Some(b'*')) => {
                    self.pos += 2;
                    depth = depth.saturating_add(1);
                }
                _ => self.pos += 1,
            }
        }
        self.push(start, self.pos, TokenClass::Comment);
        Open::Comment(depth)
    }

    /// The class of what `quote` closes: a string, or a quoted name.
    fn quote_class(&self, quote: u8) -> TokenClass {
        match quote {
            b'\'' => TokenClass::String,
            b'"' if self.dialect == Dialect::MySql => TokenClass::String,
            _ => TokenClass::Key,
        }
    }

    /// A string or a quoted name from `start`, `pos` inside it: on past its closing `quote` - a
    /// doubled one is part of it, and with `backslash` an escaped one - or to the line's end.
    /// A name in `` ` `` or `]` ends with its line.
    pub(crate) fn quoted(&mut self, start: usize, quote: u8, backslash: bool) -> Open {
        let class = self.quote_class(quote);
        while let Some(byte) = self.byte(self.pos) {
            if backslash && byte == b'\\' {
                self.pos = (self.pos + 2).min(self.line.len());
            } else if byte == quote && self.byte(self.pos + 1) == Some(quote) {
                self.pos += 2;
            } else if byte == quote {
                self.pos += 1;
                self.push(start, self.pos, class);
                return Open::Code;
            } else {
                self.pos += 1;
            }
        }
        self.push(start, self.pos, class);
        if matches!(quote, b'`' | b']') {
            Open::Code
        } else {
            Open::Quote { quote, backslash }
        }
    }

    /// A dollar-quoted string from `start`, `pos` inside it: on past `$tag$`, or to the line's
    /// end.
    pub(crate) fn dollar(&mut self, start: usize, tag: Tag) -> Open {
        while self.pos < self.line.len() {
            if self.byte(self.pos) == Some(b'$') && self.closes(tag) {
                self.pos += tag.as_bytes().len() + 2;
                self.push(start, self.pos, TokenClass::String);
                return Open::Code;
            }
            self.pos += 1;
        }
        self.push(start, self.pos, TokenClass::String);
        Open::Dollar(tag)
    }

    /// Whether `$tag$` stands at `pos`.
    fn closes(&self, tag: Tag) -> bool {
        let tag = tag.as_bytes();
        let after = self.pos + 1;
        self.line.get(after..after + tag.len()) == Some(tag)
            && self.byte(after + tag.len()) == Some(b'$')
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
