//! Strings, inside which the lexer reads text: every prefix (`r`, `b`, `u`, `f`, `t` and their
//! pairs, in any case), one quote or three, and the replacement fields of f-strings with their
//! format specs. A triple-quoted string goes on over lines, a one-quote string only after a
//! backslash at the end of a line.

use super::frames::{Frame, Quote};
use super::{Emit, Lexer, Mode};
use crate::types::TokenClass;

impl<E: Emit> Lexer<'_, E> {
    /// A string opens at `start`: `prefix` bytes of prefix letters, then its quote.
    pub(super) fn open_string(&mut self, start: usize, prefix: usize) {
        let letters = &self.line[start..start + prefix];
        let has = |wanted: u8| {
            letters
                .iter()
                .any(|byte| byte.eq_ignore_ascii_case(&wanted))
        };
        let at = start + prefix;
        let byte = self.byte(at).unwrap_or(b'"');
        let quote = Quote {
            double: byte == b'"',
            triple: self.line.get(at..at + 3) == Some([byte; 3].as_slice()),
            raw: has(b'r'),
            fields: has(b'f') || has(b't'),
        };
        let width = prefix + if quote.triple { 3 } else { 1 };
        self.mode = Mode::Expr;
        self.pos = start + width;
        if self.frames.full() {
            self.flat_string(start, quote);
            return;
        }
        self.frames.push(Frame::Str(quote));
        self.emit
            .opened(start, u8::try_from(width).unwrap_or(u8::MAX));
        self.run_from = Some(start);
    }

    /// Text of a string up to its closing quote, a replacement field, or the line's end.
    pub(super) fn string(&mut self, quote: Quote) {
        let start = self.run_from.take().unwrap_or(self.pos);
        let close = quote.byte();
        let mut at = self.pos;
        while let Some(byte) = self.byte(at) {
            if let Some(end) = self.closer_end(at, quote) {
                self.push(start, end, TokenClass::String);
                self.pos = end;
                self.close_string();
                return;
            }
            match byte {
                b'\\' => match self.byte(at + 1) {
                    None => {
                        self.string_continued = true;
                        at += 1;
                    }
                    // A named character, `\N{BULLET}`: its braces are no field.
                    Some(b'N') if !quote.raw && self.byte(at + 2) == Some(b'{') => {
                        let name = self.line[at..]
                            .iter()
                            .position(|byte| *byte == b'}' || *byte == close);
                        at = match name {
                            Some(end) if self.line[at + end] == b'}' => at + end + 1,
                            _ => at + 2,
                        };
                    }
                    Some(_) => at += 2,
                },
                b'{' | b'}' if quote.fields && self.byte(at + 1) == Some(byte) => at += 2,
                b'{' if quote.fields && !self.frames.full() => {
                    self.push(start, at, TokenClass::String);
                    self.open_field(at);
                    return;
                }
                _ => at += 1,
            }
        }
        self.push(start, self.line.len(), TokenClass::String);
        self.pos = self.line.len();
    }

    /// A format spec up to the field's `}` or a nested field's `{`.
    pub(super) fn spec(&mut self) {
        let start = self.pos;
        let mut at = self.pos;
        while let Some(byte) = self.byte(at) {
            match byte {
                b'{' if !self.frames.full() => {
                    self.push(start, at, TokenClass::String);
                    self.open_field(at);
                    return;
                }
                b'}' => {
                    self.push(start, at, TokenClass::String);
                    self.push(at, at + 1, TokenClass::Punctuation);
                    self.pos = at + 1;
                    self.frames.pop();
                    self.mode = Mode::Expr;
                    return;
                }
                _ => at += 1,
            }
        }
        self.push(start, self.line.len(), TokenClass::String);
        self.pos = self.line.len();
    }

    /// The `{` at `at` opens a replacement field: code up to its `}`.
    fn open_field(&mut self, at: usize) {
        self.push(at, at + 1, TokenClass::Punctuation);
        self.pos = at + 1;
        self.frames.push(Frame::Field);
        self.mode = Mode::Expr;
    }

    /// A string past the frames kept by kind (strings in fields in strings, eight deep): read
    /// to its closing quote on this line, without fields.
    fn flat_string(&mut self, start: usize, quote: Quote) {
        let mut at = self.pos;
        let end = loop {
            if let Some(end) = self.closer_end(at, quote) {
                break end;
            }
            match self.byte(at) {
                None => break self.line.len(),
                Some(b'\\') => at += 2,
                Some(_) => at += 1,
            }
        };
        let end = end.min(self.line.len());
        self.push(start, end, TokenClass::String);
        self.pos = end;
    }

    /// Where the string closes when its closing quote, one or three, stands at `at`.
    fn closer_end(&self, at: usize, quote: Quote) -> Option<usize> {
        let width = if quote.triple { 3 } else { 1 };
        let closer = self.line.get(at..at + width)?;
        closer
            .iter()
            .all(|byte| *byte == quote.byte())
            .then_some(at + width)
    }
}

#[cfg(test)]
#[path = "strings_tests.rs"]
mod tests;
