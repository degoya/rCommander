//! Protocol Buffers' code: `//` and `/* */` comments, `"..."` and `'...'` strings, numbers,
//! dotted names, and what each name is - a keyword, a field's or option's name (`=` or `:`
//! follows), a declared message, enum or service, an `rpc`, a field's type (a name follows it),
//! a type inside `map<...>` or an `rpc`'s brackets.

use super::words::{is_blank, is_digit, is_name, is_name_start, proto_class, proto_declares};
use super::{Emit, Lexer, Mode, Open};
use crate::types::TokenClass;

impl<E: Emit> Lexer<'_, E> {
    /// Code from `pos` until a block comment stays open at the line's end, or the line ends.
    pub(super) fn proto(&mut self) -> Open {
        while let Some(byte) = self.byte(self.pos) {
            let at = self.pos;
            let next = self.byte(at + 1);
            match byte {
                _ if is_blank(byte) => self.pos += 1,
                b'/' if next == Some(b'/') => self.rest_of_line(at, TokenClass::Comment),
                b'/' if next == Some(b'*') => {
                    self.pos = at + 2;
                    if self.block_comment(at) == Open::Comment {
                        return Open::Comment;
                    }
                }
                b'"' | b'\'' => self.quoted(at, byte),
                _ if is_digit(byte) || (byte == b'.' && next.is_some_and(is_digit)) => {
                    self.number(at);
                }
                _ if is_name_start(byte) => self.proto_word(at),
                b'(' => {
                    self.mark(at, 1, TokenClass::Punctuation);
                    self.params = self.signature;
                }
                b')' => {
                    self.mark(at, 1, TokenClass::Punctuation);
                    self.params = false;
                }
                b'<' => {
                    self.mark(at, 1, TokenClass::Punctuation);
                    self.angle = true;
                }
                b'>' => {
                    self.mark(at, 1, TokenClass::Punctuation);
                    self.angle = false;
                }
                b'=' | b'-' | b'+' => self.mark(at, 1, TokenClass::Operator),
                b'{' | b'}' | b'[' | b']' | b';' | b',' | b'.' | b':' => {
                    self.mark(at, 1, TokenClass::Punctuation);
                }
                _ => self.pos += 1,
            }
        }
        Open::Code
    }

    /// The end of the dotted name from `at`: `google.protobuf.Timestamp` is one.
    fn dotted_end(&self, at: usize) -> usize {
        let mut end = self.run(at, is_name);
        while self.byte(end) == Some(b'.') && self.byte(end + 1).is_some_and(is_name_start) {
            end = self.run(end + 1, is_name);
        }
        end
    }

    /// The (dotted) name at `at`.
    fn proto_word(&mut self, at: usize) {
        let end = self.dotted_end(at);
        let mode = std::mem::take(&mut self.mode);
        self.pos = end;
        let word = self.line.get(at..end).unwrap_or_default();
        let next = self.byte(self.skip_blanks(end));
        let class = match mode {
            Mode::TypeName => Some(TokenClass::TypeName),
            Mode::FunctionName => Some(TokenClass::Function),
            // `syntax = "proto3";` and `edition = "2023";` head the file: keywords.
            Mode::Expr if matches!(word, b"syntax" | b"edition") => Some(TokenClass::Keyword),
            // A field's name before its number, an option's before its value, a field of an
            // aggregate option (`get: "/v1/..."`) - whatever word it is.
            Mode::Expr if matches!(next, Some(b'=' | b':')) => Some(TokenClass::Key),
            Mode::Expr => match proto_class(word) {
                Some(TokenClass::Keyword) => {
                    self.mode = proto_declares(word);
                    self.signature |= word == b"rpc";
                    Some(TokenClass::Keyword)
                }
                Some(class) => Some(class),
                // A field's type: its name follows (`Timestamp created = 2;`).
                None if self.angle || self.params || next.is_some_and(is_name_start) => {
                    Some(TokenClass::TypeName)
                }
                None => None,
            },
        };
        if let Some(class) = class {
            self.push(at, end, class);
        }
    }
}
