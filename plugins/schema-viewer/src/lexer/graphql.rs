//! GraphQL's code: `#` comments, `"..."` and block strings `"""..."""`, numbers, `$variables`,
//! `@directives`, fragment spreads, and what each name is - a field's, argument's or alias's
//! name (`:` follows), a keyword, a declared type or operation, a field with arguments, a
//! built-in scalar or a type named in `PascalCase`.

use super::words::{graphql_class, graphql_declares, is_blank, is_digit, is_name, is_name_start};
use super::{Emit, Lexer, Mode, Open};
use crate::types::TokenClass;

/// Whether `word` is a type's name by convention: an upper-case letter first, a lower-case one
/// after it (`Episode`, `DateTime`) - not an enum value (`NEWHOPE`).
fn pascal_case(word: &[u8]) -> bool {
    word.first().is_some_and(u8::is_ascii_uppercase) && word.iter().any(u8::is_ascii_lowercase)
}

impl<E: Emit> Lexer<'_, E> {
    /// Code from `pos` until a block string stays open at the line's end, or the line ends.
    pub(super) fn graphql(&mut self) -> Open {
        while let Some(byte) = self.byte(self.pos) {
            let at = self.pos;
            let next = self.byte(at + 1);
            match byte {
                _ if is_blank(byte) => self.pos += 1,
                b'#' => self.rest_of_line(at, TokenClass::Comment),
                b'"' if next == Some(b'"') && self.byte(at + 2) == Some(b'"') => {
                    self.pos = at + 3;
                    if self.block_string(at) == Open::BlockString {
                        return Open::BlockString;
                    }
                }
                b'"' => self.quoted(at, byte),
                b'-' if next.is_some_and(is_digit) => self.number(at),
                _ if is_digit(byte) => self.number(at),
                b'$' if next.is_some_and(is_name_start) => {
                    let end = self.run(at + 1, is_name);
                    self.mark(at, end - at, TokenClass::Key);
                }
                b'@' if next.is_some_and(is_name_start) => {
                    let end = self.run(at + 1, is_name);
                    self.mark(at, end - at, TokenClass::Function);
                }
                b'.' if next == Some(b'.') && self.byte(at + 2) == Some(b'.') => {
                    // A fragment spread names a fragment, an inline fragment `on` a type.
                    self.mark(at, 3, TokenClass::Operator);
                    self.mode = Mode::FunctionName;
                }
                _ if is_name_start(byte) => self.graphql_word(at),
                b'&' | b'|' => {
                    // `implements A & B`, `union U = A | B`: the next name is a type.
                    self.mark(at, 1, TokenClass::Operator);
                    self.mode = Mode::TypeName;
                }
                b'=' => {
                    self.mark(at, 1, TokenClass::Operator);
                    if self.union {
                        self.mode = Mode::TypeName;
                    }
                }
                b'!' => self.mark(at, 1, TokenClass::Operator),
                // A comma is as insignificant as a blank, but shown as what it is.
                b'{' | b'}' | b'(' | b')' | b'[' | b']' | b':' | b',' => {
                    self.mark(at, 1, TokenClass::Punctuation);
                }
                _ => self.pos += 1,
            }
        }
        Open::Code
    }

    /// The name at `at`.
    fn graphql_word(&mut self, at: usize) {
        let end = self.run(at, is_name);
        let mode = std::mem::take(&mut self.mode);
        self.pos = end;
        let word = self.line.get(at..end).unwrap_or_default();
        let next = self.byte(self.skip_blanks(end));
        let class = if next == Some(b':') {
            // A field, an argument, an alias, an object's field - whatever word it is.
            Some(TokenClass::Key)
        } else if let Some(class) = graphql_class(word) {
            if class == TokenClass::Keyword {
                self.mode = graphql_declares(word);
                self.union |= word == b"union";
            }
            Some(class)
        } else {
            match mode {
                Mode::TypeName => Some(TokenClass::TypeName),
                Mode::FunctionName => Some(TokenClass::Function),
                Mode::Expr if next == Some(b'(') => Some(TokenClass::Function),
                Mode::Expr if pascal_case(word) => Some(TokenClass::TypeName),
                Mode::Expr => None,
            }
        };
        if let Some(class) = class {
            self.push(at, end, class);
        }
    }
}
