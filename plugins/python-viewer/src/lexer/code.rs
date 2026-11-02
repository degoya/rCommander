//! Code: statements and expressions - everything outside string text, also inside the
//! replacement fields of f-strings.

use super::frames::Frame;
use super::{Emit, Lexer, Mode, words};
use crate::types::TokenClass;

impl<E: Emit> Lexer<'_, E> {
    /// One step through code.
    pub(super) fn code(&mut self) {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            self.run_from = None;
            return;
        };
        let next = self.byte(at + 1);
        let in_field = self.frames.top() == Some(Frame::Field);
        match byte {
            _ if words::is_blank(byte) => self.pos += 1,
            b'#' => {
                self.push(at, self.line.len(), TokenClass::Comment);
                self.pos = self.line.len();
            }
            b'\\' => {
                // At the end of the line the statement goes on in the next.
                self.continued = next.is_none();
                self.pos += 1;
            }
            b'"' | b'\'' => self.open_string(at, 0),
            _ if words::is_name_start(byte) => self.word(),
            _ if byte.is_ascii_digit()
                || (byte == b'.' && next.is_some_and(|n| n.is_ascii_digit())) =>
            {
                let end = words::number_end(self.line, at);
                self.push(at, end, TokenClass::Number);
                self.pos = end;
                self.mode = Mode::Expr;
            }
            b'@' if self.mode == Mode::Start => self.decorator(),
            b'(' | b'[' | b'{' => {
                self.punctuation(1, Mode::Expr);
                self.frames.open_bracket();
            }
            b'}' if in_field => {
                // The field closes: the string goes on.
                self.punctuation(1, Mode::Expr);
                self.frames.pop();
            }
            b')' | b']' | b'}' => {
                self.punctuation(1, Mode::Expr);
                self.frames.close_bracket();
            }
            b':' if in_field => {
                // The format spec starts.
                self.punctuation(1, Mode::Expr);
                self.frames.field_to_spec();
            }
            b'!' if in_field && next != Some(b'=') => {
                // A conversion: `!r`, `!s`, `!a`.
                let end = words::name_end(self.line, at + 1);
                self.push(at, end, TokenClass::Operator);
                self.pos = end;
            }
            b';' => {
                let mode = if self.frames.depth() == 0 {
                    Mode::Start
                } else {
                    Mode::Expr
                };
                self.punctuation(1, mode);
            }
            b'.' if self.line[at..].starts_with(b"...") => self.punctuation(3, Mode::Expr),
            b'.' => self.punctuation(1, Mode::Attr),
            b':' if next == Some(b'=') => self.operator(2),
            b',' | b':' => self.punctuation(1, Mode::Expr),
            _ => match words::operator_width(&self.line[at..]) {
                Some(width) => self.operator(width),
                None => self.pos += 1,
            },
        }
    }

    /// `width` bytes of punctuation; the next word stands at `mode`.
    fn punctuation(&mut self, width: usize, mode: Mode) {
        self.push(self.pos, self.pos + width, TokenClass::Punctuation);
        self.pos += width;
        self.mode = mode;
    }

    /// `width` bytes of an operator.
    fn operator(&mut self, width: usize) {
        self.push(self.pos, self.pos + width, TokenClass::Operator);
        self.pos += width;
        self.mode = Mode::Expr;
    }

    /// `@` where a statement starts: a decorator, `@` and its dotted name.
    fn decorator(&mut self) {
        let at = self.pos;
        let mut end = at + 1;
        while self.byte(end).is_some_and(words::is_name_start) {
            end = words::name_end(self.line, end);
            if self.byte(end) == Some(b'.') && self.byte(end + 1).is_some_and(words::is_name_start)
            {
                end += 1;
            } else {
                break;
            }
        }
        if end == at + 1 {
            self.operator(1);
            return;
        }
        self.push(at, end, TokenClass::Function);
        self.pos = end;
        self.mode = Mode::Expr;
    }

    /// A name, or the prefix of the string right after it.
    fn word(&mut self) {
        let at = self.pos;
        let end = words::name_end(self.line, at);
        let word = &self.line[at..end];
        if matches!(self.byte(end), Some(b'"' | b'\'')) && words::is_string_prefix(word) {
            self.open_string(at, end - at);
            return;
        }
        self.pos = end;
        let mode = std::mem::replace(&mut self.mode, Mode::Expr);
        // A keyword is one wherever it stands: `from . import x`.
        let class = if words::is_keyword(word) {
            self.mode = match word {
                b"def" => Mode::DefName,
                b"class" => Mode::ClassName,
                _ => Mode::Expr,
            };
            Some(TokenClass::Keyword)
        } else {
            match mode {
                Mode::DefName => Some(TokenClass::Function),
                Mode::ClassName => Some(TokenClass::TypeName),
                Mode::Attr => None,
                Mode::Start | Mode::Expr => self.name_class(word, mode == Mode::Start, end),
            }
        };
        if let Some(class) = class {
            self.push(at, end, class);
        }
    }

    /// What a name that is no keyword is where an expression or a statement starts.
    fn name_class(&mut self, word: &[u8], start: bool, end: usize) -> Option<TokenClass> {
        if words::is_literal(word) {
            return Some(TokenClass::Literal);
        }
        if start && words::is_soft_keyword(self.line, word, end) {
            if word == b"type" {
                self.mode = Mode::ClassName;
            }
            return Some(TokenClass::Keyword);
        }
        words::builtin_class(word)
    }
}
