//! What every script lexer reads the same way: the cursor over a line (or the part of it an
//! interpolation holds), names, numbers, blanks, operators and brackets.

use super::Emit;
use crate::types::TokenClass;

/// A lexer's place in its line, and where it reports to.
pub(crate) struct Cursor<'a, E: Emit> {
    pub(crate) line: &'a [u8],
    /// Where this lexer stops: the line's end, or an interpolation's closing brace.
    pub(crate) end: usize,
    pub(crate) pos: usize,
    /// How deep inside interpolations: a string opened in one is not reported, it cannot reach
    /// past its brace.
    pub(crate) nest: u8,
    /// The last token was a value: `/` divides and `%` is modulo rather than opening a string.
    pub(crate) value: bool,
    pub(crate) emit: &'a mut E,
}

impl<'a, E: Emit> Cursor<'a, E> {
    pub(crate) fn new(line: &'a [u8], emit: &'a mut E) -> Self {
        Self {
            line,
            end: line.len(),
            pos: 0,
            nest: 0,
            value: false,
            emit,
        }
    }

    /// A cursor over `start..end` of the same line, one interpolation deeper.
    pub(crate) fn inner(&mut self, start: usize, end: usize) -> Cursor<'_, E> {
        Cursor {
            line: self.line,
            end,
            pos: start,
            nest: self.nest.saturating_add(1),
            value: false,
            emit: &mut *self.emit,
        }
    }

    /// The byte at `at`, if it lies before the end.
    pub(crate) fn byte(&self, at: usize) -> Option<u8> {
        if at < self.end {
            self.line.get(at).copied()
        } else {
            None
        }
    }

    /// The bytes `at..at + text.len()` are `text`.
    pub(crate) fn looking_at(&self, at: usize, text: &[u8]) -> bool {
        at + text.len() <= self.end && self.line.get(at..at + text.len()) == Some(text)
    }

    pub(crate) fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        self.emit.token(start, end.min(self.end), class);
    }

    /// The rest of the line is one token of `class`.
    pub(crate) fn rest(&mut self, class: TokenClass) {
        self.push(self.pos, self.end, class);
        self.pos = self.end;
    }

    /// A string opens at `col`, `width` bytes wide; not reported inside an interpolation.
    pub(crate) fn opened(&mut self, col: usize, width: usize) {
        if self.nest == 0 {
            self.emit
                .opened(col, u8::try_from(width).unwrap_or(u8::MAX));
        }
    }

    pub(crate) fn closed(&mut self) {
        if self.nest == 0 {
            self.emit.closed();
        }
    }

    pub(crate) fn failed(&mut self) {
        if self.nest == 0 {
            self.emit.failed();
        }
    }

    /// The end of the name that starts at `at`.
    pub(crate) fn name_end(&self, at: usize) -> usize {
        let mut end = at;
        while self.byte(end).is_some_and(is_name) {
            end += 1;
        }
        end
    }

    /// The end of the blanks from `at`.
    pub(crate) fn blank_end(&self, at: usize) -> usize {
        let mut end = at;
        while self
            .byte(end)
            .is_some_and(|byte| byte == b' ' || byte == b'\t')
        {
            end += 1;
        }
        end
    }

    /// The number at `pos` as one token: a prefix (`0x`, `0b`, `0o`, `&H` is VBScript's own),
    /// digits with `_`, a fraction, an exponent (`e`, or `p` after `0x`) and the letters after it
    /// (Ruby's `3r`, `2i`).
    pub(crate) fn number(&mut self) {
        let start = self.pos;
        let mut at = start;
        let hex = self.byte(at) == Some(b'0') && matches!(self.byte(at + 1), Some(b'x' | b'X'));
        if self.byte(at) == Some(b'0')
            && matches!(
                self.byte(at + 1),
                Some(b'x' | b'X' | b'b' | b'B' | b'o' | b'O' | b'd' | b'D')
            )
        {
            at += 2;
        }
        let digit = |byte: u8| {
            byte == b'_'
                || if hex {
                    byte.is_ascii_hexdigit()
                } else {
                    byte.is_ascii_digit()
                }
        };
        while self.byte(at).is_some_and(digit) {
            at += 1;
        }
        if self.byte(at) == Some(b'.') && self.byte(at + 1).is_some_and(|b| b.is_ascii_digit()) {
            at += 1;
            while self.byte(at).is_some_and(digit) {
                at += 1;
            }
        }
        let exponent = if hex { *b"pP" } else { *b"eE" };
        if self.byte(at).is_some_and(|byte| exponent.contains(&byte)) {
            let sign = usize::from(matches!(self.byte(at + 1), Some(b'+' | b'-')));
            if self.byte(at + 1 + sign).is_some_and(|b| b.is_ascii_digit()) {
                at += 1 + sign;
            }
        }
        while self
            .byte(at)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            at += 1;
        }
        self.push(start, at, TokenClass::Number);
        self.pos = at;
        self.value = true;
    }

    /// A bracket, a separator or a run of operator bytes at `pos`; any other byte is passed over
    /// uncoloured.
    pub(crate) fn symbol(&mut self) {
        let start = self.pos;
        let Some(byte) = self.byte(start) else {
            return;
        };
        match byte {
            b'(' | b'[' | b'{' | b',' | b';' => {
                self.push(start, start + 1, TokenClass::Punctuation);
                self.pos += 1;
                self.value = false;
            }
            b')' | b']' | b'}' => {
                self.push(start, start + 1, TokenClass::Punctuation);
                self.pos += 1;
                self.value = true;
            }
            b'.' if self.byte(start + 1) != Some(b'.') => {
                self.push(start, start + 1, TokenClass::Punctuation);
                self.pos += 1;
                self.value = false;
            }
            _ if is_operator(byte) => {
                let mut at = start + 1;
                while self.byte(at).is_some_and(is_operator) {
                    at += 1;
                }
                self.push(start, at, TokenClass::Operator);
                self.pos = at;
                self.value = false;
            }
            _ => self.pos += 1,
        }
    }
}

/// A byte a name starts with: a letter, `_`, or any byte of a character beyond ASCII.
pub(crate) fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// A byte of a name.
pub(crate) fn is_name(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit()
}

/// A byte of an operator.
pub(crate) fn is_operator(byte: u8) -> bool {
    matches!(
        byte,
        b'+' | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'='
            | b'<'
            | b'>'
            | b'!'
            | b'&'
            | b'|'
            | b'^'
            | b'~'
            | b'?'
            | b':'
            | b'.'
            | b'\\'
    )
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
