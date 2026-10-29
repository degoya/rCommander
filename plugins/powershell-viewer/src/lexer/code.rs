//! Code: statements, commands with their arguments, expressions - everything outside strings
//! and comments.

use super::frames::Frame;
use super::{Emit, Lexer, Mode, words};
use crate::types::TokenClass;

impl<E: Emit> Lexer<'_, E> {
    /// One step through code.
    pub(super) fn code(&mut self) {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            return;
        };
        let next = self.byte(at + 1);
        match byte {
            _ if words::is_blank(byte) => self.pos += 1,
            b'#' => {
                self.push(at, self.line.len(), TokenClass::Comment);
                self.pos = self.line.len();
            }
            b'<' if next == Some(b'#') => self.open(Frame::Comment, 2),
            b'`' => self.backtick(),
            b'"' => {
                self.after_value();
                self.open(Frame::DQuote, 1);
            }
            b'\'' => {
                self.after_value();
                self.open(Frame::SQuote, 1);
            }
            b'$' => self.dollar(),
            b'@' => self.at_sign(),
            b'(' => self.open_paren(1),
            b')' => self.close_paren(),
            b'{' | b'}' => self.punctuation(1, Mode::Command),
            b'[' => self.bracket(),
            b']' => {
                let mode = if self.mode == Mode::Args {
                    Mode::Args
                } else {
                    Mode::Expr
                };
                self.punctuation(1, mode);
            }
            b',' => {
                let mode = if self.mode == Mode::Args {
                    Mode::Args
                } else {
                    Mode::Command
                };
                self.punctuation(1, mode);
            }
            // `=` alone, also among a command's arguments: `@{ Name = 'x' }`, `Mandatory=$true`.
            b';' | b'=' => self.operator(1, Mode::Command),
            b'|' | b'&' => {
                let width = if next == Some(byte) { 2 } else { 1 };
                self.operator(width, Mode::Command);
            }
            b'-' if next.is_some_and(|byte| byte.is_ascii_alphabetic()) => {
                // A parameter (`-Path`) or an operator (`-eq`, `-f`, `-not`).
                let end = words::name_end(self.line, at + 1);
                self.push(at, end, TokenClass::Operator);
                self.pos = end;
                if self.mode != Mode::Args {
                    self.mode = Mode::Command;
                }
            }
            _ if words::redirect_end(self.line, at).is_some() => {
                let end = words::redirect_end(self.line, at).unwrap_or(at + 1);
                self.push(at, end, TokenClass::Operator);
                self.pos = end;
            }
            _ if self.mode == Mode::Args => self.argument(),
            _ => self.expression(),
        }
    }

    /// `width` bytes of punctuation; the next word stands at `mode`.
    fn punctuation(&mut self, width: usize, mode: Mode) {
        self.push(self.pos, self.pos + width, TokenClass::Punctuation);
        self.pos += width;
        self.mode = mode;
    }

    /// `width` bytes of an operator; the next word stands at `mode`.
    fn operator(&mut self, width: usize, mode: Mode) {
        self.push(self.pos, self.pos + width, TokenClass::Operator);
        self.pos += width;
        self.mode = mode;
    }

    /// `$`: a variable, `$(`, or a `$` that is neither.
    fn dollar(&mut self) {
        if self.byte(self.pos + 1) == Some(b'(') {
            self.open_paren(2);
            return;
        }
        match words::variable_end(self.line, self.pos) {
            Some(end) => {
                let class = words::variable_class(&self.line[self.pos..end]);
                self.push(self.pos, end, class);
                self.pos = end;
                self.after_value();
            }
            None => self.pos += 1,
        }
    }

    /// `@`: `@(`, `@{`, a here-string, splatting (`@args`), or a `@` alone.
    fn at_sign(&mut self) {
        match self.byte(self.pos + 1) {
            Some(b'(') => self.open_paren(2),
            Some(b'{') => self.punctuation(2, Mode::Command),
            Some(b'"' | b'\'') if self.here_start() => {}
            Some(next) if words::is_name_start(next) => {
                let end = words::name_end(self.line, self.pos + 1);
                self.push(self.pos, end, TokenClass::Key);
                self.pos = end;
                self.after_value();
            }
            _ => self.punctuation(1, self.mode),
        }
    }

    /// `[`: a type (`[string]`, `[int[]]`, an attribute's `[CmdletBinding(`) where a value
    /// starts, else an index.
    fn bracket(&mut self) {
        let indexes = self
            .pos
            .checked_sub(1)
            .and_then(|before| self.byte(before))
            .is_some_and(|before| words::is_name(before) || matches!(before, b')' | b']' | b'}'));
        if self.mode == Mode::Args || indexes || !self.type_literal() {
            let mode = if self.mode == Mode::Args {
                Mode::Args
            } else {
                Mode::Command
            };
            self.punctuation(1, mode);
        }
    }

    /// A type literal from the `[` at `pos`: its names `type-name`, its brackets and commas
    /// punctuation. Stops after its last `]`, or at an attribute's `(`, which the code after
    /// reads. False when no name follows the `[`.
    fn type_literal(&mut self) -> bool {
        if !self.byte(self.pos + 1).is_some_and(words::is_name_start) {
            return false;
        }
        self.push(self.pos, self.pos + 1, TokenClass::Punctuation);
        let mut at = self.pos + 1;
        let mut depth = 1u32;
        while let Some(byte) = self.byte(at) {
            if words::is_type_name(byte) {
                let length = self.line[at..]
                    .iter()
                    .take_while(|byte| words::is_type_name(**byte))
                    .count();
                self.push(at, at + length, TokenClass::TypeName);
                at += length;
                continue;
            }
            match byte {
                b'[' | b',' => {
                    self.push(at, at + 1, TokenClass::Punctuation);
                    depth += u32::from(byte == b'[');
                    at += 1;
                }
                b']' => {
                    self.push(at, at + 1, TokenClass::Punctuation);
                    at += 1;
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ if words::is_blank(byte) => at += 1,
                _ => break,
            }
        }
        self.pos = at;
        self.mode = Mode::Expr;
        true
    }

    /// A bare word among a command's arguments: text, a number when it is one.
    fn argument(&mut self) {
        let at = self.pos;
        let end = words::word_end(self.line, at, true);
        if words::is_number(&self.line[at..end]) {
            self.push(at, end, TokenClass::Number);
        }
        self.pos = end;
    }

    /// One step where a statement starts or an expression is read.
    fn expression(&mut self) {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            return;
        };
        let next = self.byte(at + 1);
        if matches!(self.mode, Mode::FuncName | Mode::TypeName) {
            let end = words::word_end(self.line, at, false);
            let class = if self.mode == Mode::FuncName {
                TokenClass::Function
            } else {
                TokenClass::TypeName
            };
            self.push(at, end, class);
            self.pos = end;
            self.mode = Mode::Expr;
            return;
        }
        let digit = |byte: Option<u8>| byte.is_some_and(|byte| byte.is_ascii_digit());
        if (digit(Some(byte)) || (byte == b'.' && digit(next)))
            && let Some(end) = words::number_end(self.line, at)
        {
            self.push(at, end, TokenClass::Number);
            self.pos = end;
            self.mode = Mode::Expr;
            return;
        }
        let command = self.mode == Mode::Command;
        match byte {
            b'.' if next == Some(b'.') => self.operator(2, Mode::Command),
            b'.' if !command && next.is_some_and(words::is_name_start) => {
                // A member: `$x.Name`, `(Get-Date).Year`.
                self.pos = words::name_end(self.line, at + 1);
            }
            b'.' if next.is_none_or(words::is_blank) => self.operator(1, Mode::Command),
            b':' if next == Some(b':') => {
                // A static member: `[Math]::Round`.
                self.push(at, at + 2, TokenClass::Operator);
                self.pos = words::name_end(self.line, at + 2);
                self.mode = Mode::Expr;
            }
            b'!' => self.operator(1, Mode::Command),
            b'+' | b'-' | b'*' | b'/' | b'%' | b'?' if !command || matches!(byte, b'+' | b'-') => {
                let width = 1 + usize::from(next == Some(b'=') || next == Some(byte));
                self.operator(width, Mode::Command);
            }
            _ if command => self.command_name(),
            _ => self.pos = words::word_end(self.line, at, false),
        }
    }

    /// The first word of a statement: a keyword, a cmdlet, or another command's name.
    fn command_name(&mut self) {
        let at = self.pos;
        let end = words::word_end(self.line, at, false);
        let word = &self.line[at..end];
        self.pos = end;
        if words::is_keyword(word) {
            self.push(at, end, TokenClass::Keyword);
            let lower = word.to_ascii_lowercase();
            self.mode = match lower.as_slice() {
                b"function" | b"filter" => Mode::FuncName,
                b"class" | b"enum" => Mode::TypeName,
                _ => Mode::Command,
            };
            return;
        }
        if words::is_verb_noun(word) {
            self.push(at, end, TokenClass::Function);
        }
        self.mode = Mode::Args;
    }
}
