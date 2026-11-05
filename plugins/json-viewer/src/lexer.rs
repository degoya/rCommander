//! The JSON lexer, one for every pass: the structure and line index of `open`, the pretty
//! printer, the tree's walks and the tokens of one line; in each `Dialect` (RC-100-80).
//!
//! No string spans a line: it may not hold a raw line break, so a string still open at the end
//! of its line ends there as `invalid`. Only a `/* */` comment (JSONC, JSON5) runs on over lines.
//! A lexer over the whole stream skips comments as whitespace and tells `on_line` which lines
//! start inside one; a lexer over one line (`Lexer::per_line`) hands them out as lexemes and
//! knows whether the line ends inside one. That flag is the whole state at a line's end, and why
//! `tokens` can start at any line whose flag the index or the lines before it give.

mod comments;

use crate::dialect::Dialect;
use crate::reader::Bytes;
use crate::types::Failure;

/// What a lexeme is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    BeginObject,
    EndObject,
    BeginArray,
    EndArray,
    Comma,
    Colon,
    String,
    Number,
    Literal,
    /// JSON5: a name without quotes, a key and never a value.
    Identifier,
    /// `//` or `/* */`; only a lexer per line hands one out.
    Comment,
    Invalid(Problem),
}

impl Kind {
    pub(crate) fn is_close(self) -> bool {
        matches!(self, Self::EndObject | Self::EndArray)
    }

    pub(crate) fn is_open(self) -> bool {
        matches!(self, Self::BeginObject | Self::BeginArray)
    }
}

/// Why a lexeme is `invalid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Problem {
    UnterminatedString,
    UnterminatedComment,
    BadEscape,
    ControlInString,
    BadNumber,
    BadLiteral,
    Unexpected,
}

impl Problem {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::UnterminatedString => "unterminated string",
            Self::UnterminatedComment => "unterminated comment",
            Self::BadEscape => "invalid escape in a string",
            Self::ControlInString => "control character in a string",
            Self::BadNumber => "invalid number",
            Self::BadLiteral => "invalid literal",
            Self::Unexpected => "unexpected character",
        }
    }
}

/// One lexeme: bytes `start..end` of its stream, and where it starts (1-based line and column in
/// characters, counted only by a lexer that started at the beginning).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lexeme {
    pub(crate) kind: Kind,
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) line: u64,
    pub(crate) column: u64,
}

/// Hears of every line a lexer starts: its 0-based number, the offset of its first byte, and
/// whether it starts inside a `/* */` comment.
pub(crate) type OnLine<'a> = dyn FnMut(u64, u64, bool) + 'a;

pub(crate) struct Lexer<B: Bytes> {
    bytes: B,
    line: u64,
    column: u64,
    capture_cap: usize,
    captured: Vec<u8>,
    dialect: Dialect,
    /// One line: comments are lexemes, and a `/* */` may go on past the end.
    per_line: bool,
    /// Per line: inside a `/* */` comment, at the line's start and, once read, at its end.
    in_block: bool,
    /// The whole stream: a `/* */` comment ran to its end, from here (offset, line, column).
    broken_comment: Option<(u64, u64, u64)>,
}

/// The number grammar, `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`, a byte at a time;
/// JSON5 adds `+`, a point at either end and `0x` digits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Num {
    Start,
    Sign,
    Zero,
    Int,
    Dot,
    LeadDot,
    Frac,
    Exp,
    ExpSign,
    ExpDigits,
    HexMark,
    Hex,
    Bad,
}

impl Num {
    fn step(self, byte: u8, json5: bool) -> Self {
        match (self, byte) {
            (Self::HexMark | Self::Hex, _) if byte.is_ascii_hexdigit() => Self::Hex,
            (Self::HexMark | Self::Hex, _) => Self::Bad,
            (Self::Start, b'-') => Self::Sign,
            (Self::Start, b'+') if json5 => Self::Sign,
            (Self::Start | Self::Sign, b'0') => Self::Zero,
            (Self::Start | Self::Sign, b'1'..=b'9') => Self::Int,
            (Self::Start | Self::Sign, b'.') if json5 => Self::LeadDot,
            (Self::Int, b'0'..=b'9') => Self::Int,
            (Self::Zero, b'x' | b'X') if json5 => Self::HexMark,
            (Self::Zero | Self::Int, b'.') => Self::Dot,
            (Self::Dot | Self::LeadDot | Self::Frac, b'0'..=b'9') => Self::Frac,
            (Self::Zero | Self::Int | Self::Frac, b'e' | b'E') => Self::Exp,
            (Self::Dot, b'e' | b'E') if json5 => Self::Exp,
            (Self::Exp, b'+' | b'-') => Self::ExpSign,
            (Self::Exp | Self::ExpSign | Self::ExpDigits, b'0'..=b'9') => Self::ExpDigits,
            _ => Self::Bad,
        }
    }

    fn accepts(self, json5: bool) -> bool {
        matches!(self, Self::Zero | Self::Int | Self::Frac | Self::ExpDigits)
            || (json5 && matches!(self, Self::Dot | Self::Hex))
    }
}

impl<B: Bytes> Lexer<B> {
    pub(crate) fn new(bytes: B) -> Self {
        Self {
            bytes,
            line: 1,
            column: 1,
            capture_cap: 0,
            captured: Vec::new(),
            dialect: Dialect::Json,
            per_line: false,
            in_block: false,
            broken_comment: None,
        }
    }

    /// Reads `dialect` rather than JSON.
    pub(crate) fn dialect(mut self, dialect: Dialect) -> Self {
        self.dialect = dialect;
        self
    }

    /// Reads one line, which starts inside a `/* */` comment when `in_comment`.
    pub(crate) fn per_line(mut self, in_comment: bool) -> Self {
        self.per_line = true;
        self.in_block = in_comment;
        self
    }

    /// Per line: whether the line read so far ends inside a `/* */` comment.
    pub(crate) fn in_block_comment(&self) -> bool {
        self.in_block
    }

    /// Keeps the first `cap` bytes of every lexeme in [`Self::captured`].
    pub(crate) fn with_capture(mut self, cap: usize) -> Self {
        self.capture_cap = cap;
        self
    }

    pub(crate) fn bytes_mut(&mut self) -> &mut B {
        &mut self.bytes
    }

    /// The first bytes of the last lexeme, at most the capture's cap of them.
    pub(crate) fn captured(&self) -> &[u8] {
        &self.captured
    }

    /// The 1-based line and column of the next byte.
    pub(crate) fn position(&self) -> (u64, u64) {
        (self.line, self.column)
    }

    fn is_whitespace(&self, byte: u8) -> bool {
        matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
            || (self.dialect.json5() && matches!(byte, 0x0B | 0x0C))
    }

    /// Ends a run of invalid bytes: whitespace, structure, a quote or a comment.
    fn is_delimiter(&self, byte: u8) -> bool {
        self.is_whitespace(byte)
            || matches!(byte, b'{' | b'}' | b'[' | b']' | b',' | b':' | b'"')
            || (self.dialect.comments() && byte == b'/')
            || (self.dialect.json5() && byte == b'\'')
    }

    fn take(&mut self, byte: u8) {
        self.bytes.bump();
        if byte & 0xC0 != 0x80 {
            self.column += 1;
        }
        if self.captured.len() < self.capture_cap {
            self.captured.push(byte);
        }
    }

    /// Takes a line break.
    fn newline(&mut self, on_line: &mut OnLine<'_>, in_comment: bool) {
        self.bytes.bump();
        self.line += 1;
        self.column = 1;
        on_line(self.line - 1, self.bytes.offset(), in_comment);
    }

    /// Skips whitespace - and, over the whole stream, comments - telling `on_line` of every line
    /// it starts, and returns the next byte.
    pub(crate) fn skip_whitespace(
        &mut self,
        on_line: &mut OnLine<'_>,
    ) -> Result<Option<u8>, Failure> {
        while let Some(byte) = self.bytes.peek()? {
            if byte == b'\n' {
                self.newline(on_line, false);
            } else if self.is_whitespace(byte) {
                self.bytes.bump();
                self.column += 1;
            } else if byte == b'/' && !self.per_line && self.dialect.comments() {
                if !self.skip_comment(on_line)? {
                    return Ok(Some(byte));
                }
            } else {
                return Ok(Some(byte));
            }
        }
        Ok(None)
    }

    pub(crate) fn next(&mut self) -> Result<Option<Lexeme>, Failure> {
        self.next_with(&mut |_, _, _| {})
    }

    /// The next lexeme; `on_line` hears of the lines started on the way (0-based line numbers).
    pub(crate) fn next_with(
        &mut self,
        on_line: &mut OnLine<'_>,
    ) -> Result<Option<Lexeme>, Failure> {
        if self.per_line && self.in_block {
            return self.continued_comment();
        }
        let Some(first) = self.skip_whitespace(on_line)? else {
            return Ok(self
                .broken_comment
                .take()
                .map(|(start, line, column)| Lexeme {
                    kind: Kind::Invalid(Problem::UnterminatedComment),
                    start,
                    end: self.bytes.offset(),
                    line,
                    column,
                }));
        };
        self.captured.clear();
        let (start, line, column) = (self.bytes.offset(), self.line, self.column);
        let json5 = self.dialect.json5();
        let kind = match first {
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                self.take(first);
                match first {
                    b'{' => Kind::BeginObject,
                    b'}' => Kind::EndObject,
                    b'[' => Kind::BeginArray,
                    b']' => Kind::EndArray,
                    b',' => Kind::Comma,
                    _ => Kind::Colon,
                }
            }
            b'"' => self.string(b'"')?,
            b'\'' if json5 => self.string(b'\'')?,
            b'/' if self.dialect.comments() => self.comment(on_line)?,
            b'-' | b'0'..=b'9' => self.number()?,
            b'+' | b'.' if json5 => self.number()?,
            b'a'..=b'z' | b'A'..=b'Z' => self.word()?,
            b'_' | b'$' | 0x80..=0xFF if json5 => self.word()?,
            _ => self.unexpected()?,
        };
        Ok(Some(Lexeme {
            kind,
            start,
            end: self.bytes.offset(),
            line,
            column,
        }))
    }

    /// A string in `quote`s; JSON5's own may be in single quotes.
    fn string(&mut self, quote: u8) -> Result<Kind, Failure> {
        self.take(quote);
        let json5 = self.dialect.json5();
        let mut problem = None;
        loop {
            let Some(byte) = self.bytes.peek()? else {
                return Ok(Kind::Invalid(Problem::UnterminatedString));
            };
            match byte {
                // Never past the line: the next one starts clean.
                b'\n' | b'\r' => return Ok(Kind::Invalid(Problem::UnterminatedString)),
                _ if byte == quote => {
                    self.take(byte);
                    return Ok(problem.map_or(Kind::String, Kind::Invalid));
                }
                b'\\' => {
                    self.take(byte);
                    if !self.escape()? {
                        problem.get_or_insert(Problem::BadEscape);
                    }
                }
                0x00..=0x1F if !json5 => {
                    self.take(byte);
                    problem.get_or_insert(Problem::ControlInString);
                }
                _ => self.take(byte),
            }
        }
    }

    /// The escape after a backslash; false when it is not one the dialect knows. Never takes a
    /// line break, so the string still ends where it should.
    fn escape(&mut self) -> Result<bool, Failure> {
        let json5 = self.dialect.json5();
        match self.bytes.peek()? {
            Some(b'u') => {
                self.take(b'u');
                self.hex_digits(4)
            }
            Some(b'x') if json5 => {
                self.take(b'x');
                self.hex_digits(2)
            }
            Some(byte @ (b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't')) => {
                self.take(byte);
                Ok(true)
            }
            // JSON5 escapes any character but a digit other than `0`.
            Some(byte) if json5 && !matches!(byte, b'\n' | b'\r') => {
                self.take(byte);
                Ok(!matches!(byte, b'1'..=b'9'))
            }
            Some(byte) if byte >= 0x20 => {
                self.take(byte);
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn hex_digits(&mut self, count: usize) -> Result<bool, Failure> {
        for _ in 0..count {
            match self.bytes.peek()? {
                Some(byte) if byte.is_ascii_hexdigit() => self.take(byte),
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    fn number(&mut self) -> Result<Kind, Failure> {
        let json5 = self.dialect.json5();
        let mut state = Num::Start;
        while let Some(byte) = self.bytes.peek()? {
            let part = matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
                || (json5 && (byte.is_ascii_hexdigit() || matches!(byte, b'x' | b'X')));
            if !part {
                break;
            }
            state = state.step(byte, json5);
            self.take(byte);
        }
        // JSON5: `+Infinity`, `-NaN`.
        if json5 && state == Num::Sign && matches!(self.bytes.peek()?, Some(b'I' | b'N')) {
            return Ok(match self.word()? {
                Kind::Number => Kind::Number,
                _ => Kind::Invalid(Problem::BadNumber),
            });
        }
        // `12abc` is one invalid run, not a number and a literal.
        if let Some(byte) = self.bytes.peek()?
            && !self.is_delimiter(byte)
        {
            self.unexpected()?;
            return Ok(Kind::Invalid(Problem::BadNumber));
        }
        Ok(if state.accepts(json5) {
            Kind::Number
        } else {
            Kind::Invalid(Problem::BadNumber)
        })
    }

    /// `true`, `false`, `null`; in JSON5 also `Infinity`, `NaN` and a name without quotes.
    fn word(&mut self) -> Result<Kind, Failure> {
        let json5 = self.dialect.json5();
        let mut word = [0u8; 8];
        let mut len = 0usize;
        while let Some(byte) = self.bytes.peek()? {
            let part = byte.is_ascii_alphanumeric()
                || (json5 && matches!(byte, b'_' | b'$' | 0x80..=0xFF));
            if !part {
                break;
            }
            if let Some(slot) = word.get_mut(len) {
                *slot = byte;
            }
            len += 1;
            self.take(byte);
        }
        if let Some(byte) = self.bytes.peek()?
            && !self.is_delimiter(byte)
        {
            self.unexpected()?;
            return Ok(Kind::Invalid(Problem::BadLiteral));
        }
        let word = word.get(..len).unwrap_or_default();
        Ok(match word {
            b"true" | b"false" | b"null" => Kind::Literal,
            b"Infinity" | b"NaN" if json5 => Kind::Number,
            _ if json5 => Kind::Identifier,
            _ => Kind::Invalid(Problem::BadLiteral),
        })
    }

    /// A run of bytes the dialect has no place for, up to the next delimiter; at least one byte.
    fn unexpected(&mut self) -> Result<Kind, Failure> {
        if let Some(byte) = self.bytes.peek()? {
            self.take(byte);
        }
        while let Some(byte) = self.bytes.peek()? {
            if self.is_delimiter(byte) {
                break;
            }
            self.take(byte);
        }
        Ok(Kind::Invalid(Problem::Unexpected))
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
