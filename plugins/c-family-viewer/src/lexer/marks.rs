//! The marks one language reads its own way: `#` (directives in C, C++ and C#, attributes and
//! a shebang in Rust), `@` (annotations in Java and Kotlin, verbatim strings and names in C#),
//! and `'` (characters everywhere, lifetimes in Rust).

use super::frames::{Str, StrKind};
use super::{Carry, Emit, Lexer, Mode, words};
use crate::lang::{Declares, Lang};
use crate::types::TokenClass;

/// The longest character literal looked for, quotes included: `'\u{10FFFF}'` is 12 bytes, C++'s
/// `U'\U0001F600'` 13. A quote without its closer in reach is `invalid`, so a line of stray
/// quotes is read once.
const MAX_CHAR: usize = 16;

impl<E: Emit> Lexer<'_, E> {
    /// `#`: a directive heading a line in C, C++ and C#, an attribute or a shebang in Rust, a
    /// shebang in a Kotlin script, else an operator (`#` and `##` in a macro).
    pub(super) fn hash(&mut self, at: usize, line_start: bool) {
        let next = self.byte(at + 1);
        if line_start && self.lang.directives() {
            self.directive(at);
            return;
        }
        if self.lang == Lang::Rust {
            let width = match (next, self.byte(at + 2)) {
                (Some(b'['), _) => Some(2),
                (Some(b'!'), Some(b'[')) => Some(3),
                _ => None,
            };
            if let Some(width) = width {
                // `#[derive(Debug)]`, `#![allow(...)]`: the attribute's name is coloured.
                self.punctuation(width, Mode::FunctionName);
                return;
            }
        }
        if line_start && next == Some(b'!') && matches!(self.lang, Lang::Rust | Lang::Kotlin) {
            self.rest_of_line(at, TokenClass::Comment);
            return;
        }
        let width = words::operator_width(&self.line[at..]).unwrap_or(1);
        self.operator(width);
    }

    /// `#include`, `# define`, `#region`: the directive's name is a keyword, the rest of the line
    /// code - but an include's `<path>`, a macro's name, and the text of `#error`, `#warning`
    /// and `#region`.
    fn directive(&mut self, at: usize) {
        let line = self.line;
        let name = words::skip_blanks(line, at + 1);
        let end = words::name_end(self.lang, line, name);
        self.push(at, end, TokenClass::Keyword);
        self.pos = end;
        self.directive = true;
        self.mode = Mode::Expr;
        match &line[name..end] {
            b"include" | b"include_next" | b"import" if self.lang != Lang::CSharp => {
                let path = words::skip_blanks(line, end);
                if self.byte(path) == Some(b'<') {
                    let close = line[path..]
                        .iter()
                        .position(|byte| *byte == b'>')
                        .map_or(line.len(), |close| path + close + 1);
                    self.push(path, close, TokenClass::String);
                    self.pos = close;
                }
            }
            b"define" => self.mode = Mode::FunctionName,
            b"error" | b"warning" | b"region" | b"endregion" => {
                self.pos = line.len();
                if self.lang.splices() && line.last() == Some(&b'\\') {
                    self.carry = Carry::DirectiveText;
                }
            }
            _ => {}
        }
    }

    /// `@`: in C# a verbatim string (`@"`, `@$"`) or name (`@class`); in Java and Kotlin an
    /// annotation with its dotted name (`@Override`, `@a.b.C`, `@interface`); else an operator.
    pub(super) fn at_sign(&mut self, at: usize) {
        let next = self.byte(at + 1);
        if self.lang == Lang::CSharp {
            match next {
                Some(b'"') => {
                    self.open_string(at, 2, Str::plain(StrKind::Verbatim));
                    return;
                }
                Some(b'$') if self.byte(at + 2) == Some(b'"') => {
                    let open = Str {
                        dollars: 1,
                        ..Str::plain(StrKind::Verbatim)
                    };
                    self.open_string(at, 3, open);
                    return;
                }
                Some(byte) if words::is_name_start(self.lang, byte) => {
                    // A keyword used as a name: no keyword.
                    self.pos = words::name_end(self.lang, self.line, at + 1);
                    self.mode = Mode::Expr;
                    return;
                }
                _ => {}
            }
        }
        if self.lang.annotations() && next.is_some_and(|byte| words::is_name_start(self.lang, byte))
        {
            let line = self.line;
            let mut end = at + 1;
            loop {
                end = words::name_end(self.lang, line, end);
                let dotted = self.byte(end) == Some(b'.')
                    && self
                        .byte(end + 1)
                        .is_some_and(|byte| words::is_name_start(self.lang, byte));
                if !dotted {
                    break;
                }
                end += 1;
            }
            let name = &line[at + 1..end];
            self.mode = Mode::Expr;
            let class = if self.lang.classify(name) == Some(TokenClass::Keyword) {
                if self.lang.declares(name) == Some(Declares::Type) {
                    self.mode = Mode::TypeName;
                }
                TokenClass::Keyword
            } else {
                TokenClass::Function
            };
            self.push(at, end, class);
            self.pos = end;
            return;
        }
        self.operator(1);
    }

    /// A character from `start` (its prefix, `L`, `u8`, `b`) with its quote at `quote`: up to
    /// the closing quote, escapes skipped. Without one in reach the quote is `invalid`.
    pub(super) fn char_literal(&mut self, start: usize, quote: usize) {
        let limit = (quote + MAX_CHAR).min(self.line.len());
        let mut at = quote + 1;
        while at < limit {
            match self.line[at] {
                b'\\' => at += 2,
                b'\'' => {
                    self.push(start, at + 1, TokenClass::String);
                    self.pos = at + 1;
                    self.mode = Mode::Expr;
                    return;
                }
                _ => at += 1,
            }
        }
        self.push(start, quote + 1, TokenClass::Invalid);
        self.pos = quote + 1;
    }

    /// Rust's `'`: a character when one character and a quote follow (`'a'`, `'\n'`, also one
    /// beyond ASCII), else a lifetime or label (`'a`, `'static`, `'outer:`) - a type's part,
    /// never a string to the line's end.
    pub(super) fn rust_quote(&mut self, at: usize) {
        match self.byte(at + 1) {
            Some(b'\\') => {
                self.char_literal(at, at);
                return;
            }
            Some(byte) if byte != b'\'' => {
                let after = at + 1 + utf8_width(byte);
                if self.byte(after) == Some(b'\'') {
                    self.push(at, after + 1, TokenClass::String);
                    self.pos = after + 1;
                    self.mode = Mode::Expr;
                    return;
                }
                if words::is_name_start(self.lang, byte) {
                    let end = words::name_end(self.lang, self.line, at + 1);
                    self.push(at, end, TokenClass::TypeName);
                    self.pos = end;
                    self.mode = Mode::Expr;
                    return;
                }
            }
            _ => {}
        }
        self.push(at, at + 1, TokenClass::Invalid);
        self.pos = at + 1;
    }
}

/// The bytes of the UTF-8 character `lead` starts.
fn utf8_width(lead: u8) -> usize {
    match lead {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}
