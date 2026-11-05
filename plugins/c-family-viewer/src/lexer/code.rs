//! Code: everything outside comment and string text, also inside the fields of interpolated
//! strings - names, numbers, operators, punctuation, and where comments, strings, characters,
//! directives, attributes and annotations start.

use super::frames::{Frame, Str, StrKind};
use super::{Carry, Emit, Lexer, Mode, words};
use crate::lang::{Declares, Lang};
use crate::types::TokenClass;

impl<E: Emit> Lexer<'_, E> {
    /// One step through code.
    pub(super) fn code(&mut self) {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            self.run_from = None;
            return;
        };
        if words::is_blank(byte) {
            self.pos += 1;
            return;
        }
        let line_start = std::mem::replace(&mut self.line_start, false);
        let next = self.byte(at + 1);
        let lang = self.lang;
        match byte {
            b'/' if next == Some(b'/') => self.line_comment(at),
            b'/' if next == Some(b'*') => self.open_comment(at),
            b'#' => self.hash(at, line_start),
            b'"' => self.open_quote(at, at, 0),
            b'\'' if lang == Lang::Rust => self.rust_quote(at),
            b'\'' => self.char_literal(at, at),
            b'`' => self.backtick(at),
            b'@' => self.at_sign(at),
            b'$' if lang == Lang::CSharp => self.dollar(at),
            _ if words::is_name_start(lang, byte) => self.word(),
            _ if byte.is_ascii_digit()
                || (byte == b'.'
                    && next.is_some_and(|n| n.is_ascii_digit())
                    && !self.after_operand(at)) =>
            {
                let end = words::number_end(lang, self.line, at, self.mode == Mode::Member);
                self.push(at, end, TokenClass::Number);
                self.pos = end;
                self.mode = Mode::Expr;
            }
            b'\\' => {
                // A directive goes on in the next line.
                if next.is_none() && self.directive {
                    self.carry = Carry::Directive;
                }
                self.pos += 1;
            }
            b'(' | b'[' | b'{' => {
                self.punctuation(1, Mode::Expr);
                self.field_depth(true);
            }
            b')' | b']' => {
                self.punctuation(1, Mode::Expr);
                self.field_depth(false);
            }
            b'}' => self.close_brace(at),
            b':' if next == Some(b':') => self.punctuation(2, Mode::Expr),
            b':' if next == Some(b'=') => self.operator(2),
            b':' if lang == Lang::CSharp && matches!(self.frames.field(), Some((0, _))) => {
                // A field's format spec starts: `{value:N2}`.
                let close = self.frames.field().map_or(1, |(_, close)| close);
                self.punctuation(1, Mode::Expr);
                self.frames.set_top(Frame::Spec(close));
            }
            b';' | b',' | b':' => self.punctuation(1, Mode::Expr),
            b'.' => match words::operator_width(&self.line[at..]) {
                Some(width) if width > 1 => self.operator(width),
                _ => self.punctuation(1, Mode::Member),
            },
            _ => match words::operator_width(&self.line[at..]) {
                Some(width) => self.operator(width),
                None => self.pos += 1,
            },
        }
    }

    /// `width` bytes of punctuation; the next word stands at `mode`.
    pub(super) fn punctuation(&mut self, width: usize, mode: Mode) {
        self.push(self.pos, self.pos + width, TokenClass::Punctuation);
        self.pos += width;
        self.mode = mode;
    }

    /// `width` bytes of an operator.
    pub(super) fn operator(&mut self, width: usize) {
        self.push(self.pos, self.pos + width, TokenClass::Operator);
        self.pos += width;
        self.mode = Mode::Expr;
    }

    /// Whether the byte before `at` ends an operand, so a `.` there is a member's, not a
    /// number's: `x.5` is no number, `t.0.1` two fields.
    fn after_operand(&self, at: usize) -> bool {
        at.checked_sub(1)
            .and_then(|before| self.byte(before))
            .is_some_and(|byte| words::is_name(self.lang, byte) || matches!(byte, b')' | b']'))
    }

    /// A bracket opens (`open`) or closes inside a field: the field's own `}` is found by them.
    fn field_depth(&mut self, open: bool) {
        if let Some((depth, close)) = self.frames.field() {
            let depth = if open {
                depth.saturating_add(1)
            } else {
                depth.saturating_sub(1)
            };
            self.frames.set_top(Frame::Field { depth, close });
        }
    }

    /// `}`: closes a field when no bracket is open in it - with as many braces as a C# raw
    /// string has `$` signs - else a block.
    fn close_brace(&mut self, at: usize) {
        match self.frames.field() {
            Some((0, close)) => {
                let close = usize::from(close.max(1));
                if close == 1 || words::run_of(self.line, at, b'}') >= close {
                    self.punctuation(close, Mode::Expr);
                    self.frames.pop();
                } else {
                    self.punctuation(1, Mode::Expr);
                }
            }
            _ => {
                self.punctuation(1, Mode::Expr);
                self.field_depth(false);
            }
        }
    }

    /// `//` to the line's end; in C and C++ a backslash ending it carries it into the next.
    fn line_comment(&mut self, at: usize) {
        self.rest_of_line(at, TokenClass::Comment);
        if self.lang.splices() && self.line.last() == Some(&b'\\') {
            self.carry = Carry::LineComment;
        }
    }

    /// `/*` opens a block comment.
    fn open_comment(&mut self, at: usize) {
        if !self.frames.push(Frame::Comment(1)) {
            self.rest_of_line(at, TokenClass::Comment);
            return;
        }
        self.pos = at + 2;
        self.run_from = Some(at);
    }

    /// `` ` ``: Go's raw string, Kotlin's quoted name (`` `is valid` ``), else nothing.
    fn backtick(&mut self, at: usize) {
        match self.lang {
            Lang::Go => self.open_string(at, 1, Str::plain(StrKind::Backtick)),
            Lang::Kotlin => {
                let rest = &self.line[at + 1..];
                self.pos = rest
                    .iter()
                    .position(|byte| *byte == b'`')
                    .map_or(self.line.len(), |close| at + close + 2);
                self.mode = Mode::Expr;
            }
            _ => self.pos += 1,
        }
    }

    /// A name: a keyword, a constant, a type, a function, or the prefix of the string or
    /// character right after it.
    fn word(&mut self) {
        let at = self.pos;
        let end = words::name_end(self.lang, self.line, at);
        if self.prefixed(at, end) {
            return;
        }
        self.pos = end;
        let mode = std::mem::take(&mut self.mode);
        let line = self.line;
        let word = &line[at..end];
        let class = match self.lang.classify(word) {
            Some(TokenClass::Keyword) => {
                self.mode = match self.lang.declares(word) {
                    Some(Declares::Function) => Mode::FunctionName,
                    Some(Declares::Type) => Mode::TypeName,
                    None => Mode::Expr,
                };
                Some(TokenClass::Keyword)
            }
            Some(class) => Some(class),
            None => match mode {
                Mode::FunctionName => Some(TokenClass::Function),
                Mode::TypeName => Some(TokenClass::TypeName),
                Mode::Expr | Mode::Member => self.call_or_macro(end),
            },
        };
        if let Some(class) = class {
            self.push(at, self.pos, class);
        }
    }

    /// A name that is no word of the language: a function when a call follows (`f(`), a Rust
    /// macro with its `!` (`println!`).
    fn call_or_macro(&mut self, end: usize) -> Option<TokenClass> {
        match self.byte(end) {
            Some(b'(') => Some(TokenClass::Function),
            Some(b'!') if self.lang == Lang::Rust && self.byte(end + 1) != Some(b'=') => {
                self.pos = end + 1;
                Some(TokenClass::Function)
            }
            _ => None,
        }
    }
}
