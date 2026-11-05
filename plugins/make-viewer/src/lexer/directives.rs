//! The directives with a grammar of their own: the conditions of `ifeq` and `ifneq`, and
//! `define` with its body up to `endef`.

use super::{Continued, Emit, Lexer, is_blank};
use crate::types::TokenClass;

impl<E: Emit> Lexer<'_, E> {
    /// `ifeq (a,b)`, `ifneq "a" "b"`: brackets and commas, quoted strings, references.
    pub(super) fn condition(&mut self, from: usize) -> Continued {
        let mut at = from;
        while let Some(byte) = self.byte(at) {
            match byte {
                b'$' => at = self.reference(at),
                b'#' => return self.comment(at),
                b'(' | b')' | b',' => {
                    self.push(at, at + 1, TokenClass::Punctuation);
                    at += 1;
                }
                b'"' | b'\'' => {
                    let close = self.line[at + 1..]
                        .iter()
                        .position(|other| *other == byte)
                        .map_or(self.line.len(), |close| at + close + 2);
                    self.push(at, close, TokenClass::String);
                    at = close;
                }
                _ => at += 1,
            }
        }
        Continued::Value
    }

    /// `define NAME [op]`: the body runs to its `endef`.
    pub(super) fn define(&mut self, from: usize) -> Continued {
        let is_op = |byte: u8| matches!(byte, b':' | b'?' | b'+' | b'!' | b'=');
        let name_end = self.run(from, |byte| !is_blank(byte) && !is_op(byte) && byte != b'#');
        self.names_until(from, name_end);
        let op = self.skip_blanks(name_end);
        let op_end = self.run(op, is_op);
        if self.line[op..op_end].ends_with(b"=") {
            self.push(op, op_end, TokenClass::Operator);
        }
        self.define = self.define.saturating_add(1);
        Continued::No
    }

    /// A line of a `define`'s body: its text, with references; `endef` closes the body and a
    /// nested `define` opens another.
    pub(super) fn define_body(&mut self) -> Continued {
        let at = self.skip_blanks(0);
        let end = self.run(at, |byte| byte.is_ascii_alphabetic());
        let stands = self
            .byte(end)
            .is_none_or(|byte| is_blank(byte) || byte == b'#');
        match self.line.get(at..end) {
            Some(b"endef") if stands => {
                self.push(at, end, TokenClass::Keyword);
                self.define = self.define.saturating_sub(1);
                if let Some(hash) = self.find_comment(end) {
                    self.comment(hash);
                }
            }
            Some(b"define") if stands => {
                self.push(at, end, TokenClass::Keyword);
                let next = self.skip_blanks(end);
                self.define(next);
            }
            _ => {
                let mut at = 0;
                while at < self.line.len() {
                    at = if self.byte(at) == Some(b'$') {
                        self.reference(at)
                    } else {
                        at + 1
                    };
                }
            }
        }
        Continued::No
    }
}
