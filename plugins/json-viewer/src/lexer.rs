//! The JSON lexer, one for every pass: the structure and line index of `open`, the pretty
//! printer, the tree's walks and the tokens of one line.
//!
//! A lexeme never spans a line: JSON has no comments and a string may not hold a raw line break,
//! so a string still open at the end of its line ends there as `invalid`. That is the whole
//! state at a line's end - none - and why `tokens` can start at any line.

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

pub(crate) struct Lexer<B: Bytes> {
    bytes: B,
    line: u64,
    column: u64,
    capture_cap: usize,
    captured: Vec<u8>,
}

/// The number grammar, `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`, a byte at a time.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Num {
    Start,
    Minus,
    Zero,
    Int,
    Dot,
    Frac,
    Exp,
    ExpSign,
    ExpDigits,
    Bad,
}

impl Num {
    fn step(self, byte: u8) -> Self {
        match (self, byte) {
            (Self::Start, b'-') => Self::Minus,
            (Self::Start | Self::Minus, b'0') => Self::Zero,
            (Self::Start | Self::Minus, b'1'..=b'9') => Self::Int,
            (Self::Int, b'0'..=b'9') => Self::Int,
            (Self::Zero | Self::Int, b'.') => Self::Dot,
            (Self::Dot | Self::Frac, b'0'..=b'9') => Self::Frac,
            (Self::Zero | Self::Int | Self::Frac, b'e' | b'E') => Self::Exp,
            (Self::Exp, b'+' | b'-') => Self::ExpSign,
            (Self::Exp | Self::ExpSign | Self::ExpDigits, b'0'..=b'9') => Self::ExpDigits,
            _ => Self::Bad,
        }
    }

    fn accepts(self) -> bool {
        matches!(self, Self::Zero | Self::Int | Self::Frac | Self::ExpDigits)
    }
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// Ends a run of invalid bytes: whitespace, structure or a quote.
fn is_delimiter(byte: u8) -> bool {
    is_whitespace(byte) || matches!(byte, b'{' | b'}' | b'[' | b']' | b',' | b':' | b'"')
}

impl<B: Bytes> Lexer<B> {
    pub(crate) fn new(bytes: B) -> Self {
        Self {
            bytes,
            line: 1,
            column: 1,
            capture_cap: 0,
            captured: Vec::new(),
        }
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

    fn take(&mut self, byte: u8) {
        self.bytes.bump();
        if byte & 0xC0 != 0x80 {
            self.column += 1;
        }
        if self.captured.len() < self.capture_cap {
            self.captured.push(byte);
        }
    }

    /// Skips whitespace, telling `on_line` the number and offset of every line it starts, and
    /// returns the next byte.
    pub(crate) fn skip_whitespace(
        &mut self,
        on_line: &mut dyn FnMut(u64, u64),
    ) -> Result<Option<u8>, Failure> {
        while let Some(byte) = self.bytes.peek()? {
            if !is_whitespace(byte) {
                return Ok(Some(byte));
            }
            self.bytes.bump();
            if byte == b'\n' {
                self.line += 1;
                self.column = 1;
                on_line(self.line - 1, self.bytes.offset());
            } else {
                self.column += 1;
            }
        }
        Ok(None)
    }

    pub(crate) fn next(&mut self) -> Result<Option<Lexeme>, Failure> {
        self.next_with(&mut |_, _| {})
    }

    /// The next lexeme; `on_line` hears of the lines started on the way (0-based line numbers).
    pub(crate) fn next_with(
        &mut self,
        on_line: &mut dyn FnMut(u64, u64),
    ) -> Result<Option<Lexeme>, Failure> {
        let Some(first) = self.skip_whitespace(on_line)? else {
            return Ok(None);
        };
        self.captured.clear();
        let (start, line, column) = (self.bytes.offset(), self.line, self.column);
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
            b'"' => self.string()?,
            b'-' | b'0'..=b'9' => self.number()?,
            b'a'..=b'z' | b'A'..=b'Z' => self.literal()?,
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

    fn string(&mut self) -> Result<Kind, Failure> {
        self.take(b'"');
        let mut problem = None;
        loop {
            let Some(byte) = self.bytes.peek()? else {
                return Ok(Kind::Invalid(Problem::UnterminatedString));
            };
            match byte {
                // Never past the line: the next one starts clean.
                b'\n' | b'\r' => return Ok(Kind::Invalid(Problem::UnterminatedString)),
                b'"' => {
                    self.take(byte);
                    return Ok(problem.map_or(Kind::String, Kind::Invalid));
                }
                b'\\' => {
                    self.take(byte);
                    if !self.escape()? {
                        problem.get_or_insert(Problem::BadEscape);
                    }
                }
                0x00..=0x1F => {
                    self.take(byte);
                    problem.get_or_insert(Problem::ControlInString);
                }
                _ => self.take(byte),
            }
        }
    }

    /// The escape after a backslash; false when it is not one JSON knows. Never takes a quote or
    /// a line break, so the string still ends where it should.
    fn escape(&mut self) -> Result<bool, Failure> {
        match self.bytes.peek()? {
            Some(byte @ (b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't')) => {
                self.take(byte);
                Ok(true)
            }
            Some(b'u') => {
                self.take(b'u');
                for _ in 0..4 {
                    match self.bytes.peek()? {
                        Some(byte) if byte.is_ascii_hexdigit() => self.take(byte),
                        _ => return Ok(false),
                    }
                }
                Ok(true)
            }
            Some(byte) if byte >= 0x20 => {
                self.take(byte);
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn number(&mut self) -> Result<Kind, Failure> {
        let mut state = Num::Start;
        while let Some(byte) = self.bytes.peek()? {
            if !matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
                break;
            }
            state = state.step(byte);
            self.take(byte);
        }
        // `12abc` is one invalid run, not a number and a literal.
        if let Some(byte) = self.bytes.peek()?
            && !is_delimiter(byte)
        {
            self.unexpected()?;
            return Ok(Kind::Invalid(Problem::BadNumber));
        }
        Ok(if state.accepts() {
            Kind::Number
        } else {
            Kind::Invalid(Problem::BadNumber)
        })
    }

    fn literal(&mut self) -> Result<Kind, Failure> {
        let mut word = [0u8; 5];
        let mut len = 0usize;
        while let Some(byte) = self.bytes.peek()? {
            if !byte.is_ascii_alphanumeric() {
                break;
            }
            if let Some(slot) = word.get_mut(len) {
                *slot = byte;
            }
            len += 1;
            self.take(byte);
        }
        if let Some(byte) = self.bytes.peek()?
            && !is_delimiter(byte)
        {
            self.unexpected()?;
            return Ok(Kind::Invalid(Problem::BadLiteral));
        }
        let word = word.get(..len).unwrap_or_default();
        Ok(if matches!(word, b"true" | b"false" | b"null") {
            Kind::Literal
        } else {
            Kind::Invalid(Problem::BadLiteral)
        })
    }

    /// A run of bytes JSON has no place for, up to the next delimiter; at least one byte.
    fn unexpected(&mut self) -> Result<Kind, Failure> {
        while let Some(byte) = self.bytes.peek()? {
            if is_delimiter(byte) {
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
