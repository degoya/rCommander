//! JavaScript and TypeScript, with JSX in `.jsx` and `.tsx`: a file of its own, a `<script>`
//! element, an `on...` attribute, a template string's `${...}` or a JSX `{...}`.
//!
//! **A regular expression or a division.** A `/` starts a regular expression where no value
//! comes before it - at the start, after an operator, `(`, `[`, `{`, `}`, `,`, `;`, `:` or a
//! keyword like `return` - and is a division after a name, a number, a string, `)` or `]`: the
//! usual heuristic, kept in the frame ([`JsFrame::regex_ok`]) so it holds across lines.
//!
//! Classes: keywords `keyword`; object keys and labels `key`; strings, template strings and
//! regular expressions `string`; numbers `number`; `true false null undefined NaN` `literal`;
//! comments `comment`; classes, TypeScript types and type arguments `type-name`; functions
//! where called or defined, decorators `function`; operators `operator`; brackets, `;`, `,`,
//! `.` and the `<` `>` of type arguments `punctuation`.

use super::scan::{StrEnd, is_blank, is_ident, is_ident_start};
use super::state::{End, Frame, JsFrame, JsSub, JsxSub};
use super::{Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

/// The operators of more than one character, the longest first.
const OPERATORS: &[&[u8]] = &[
    b">>>=", b"...", b"===", b"!==", b"**=", b"<<=", b">>=", b">>>", b"&&=", b"||=", b"??=", b"=>",
    b"==", b"!=", b"<=", b">=", b"&&", b"||", b"??", b"?.", b"++", b"--", b"+=", b"-=", b"*=",
    b"/=", b"%=", b"&=", b"|=", b"^=", b"**", b"<<", b">>",
];

/// What one line of JavaScript keeps for itself, for the next word.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Scratch {
    /// After `.` or `?.`: a property.
    pub(super) after_dot: bool,
    /// After `class`, `new`, `extends`, `as` and the like: a type.
    pub(super) type_next: bool,
    /// After a TypeScript `:` or a type's `|` or `&`: a built-in or capitalized name is a type.
    pub(super) annot: bool,
    /// The last word was a type: a `|` or `&` goes on with another.
    pub(super) in_type: bool,
    /// After `case`: the `:` ahead ends the case, it makes no key.
    pub(super) after_case: bool,
    /// The `?` of conditions whose `:` is still to come.
    pub(super) ternary: u8,
    /// Type arguments end at this source offset: their names are types, their `<` and `>`
    /// punctuation.
    pub(super) generic_end: u64,
}

impl<S: Source> Lexer<'_, S> {
    pub(super) fn js(
        &mut self,
        mut frame: JsFrame,
        byte: u8,
        start: u32,
    ) -> Result<Lexeme, Failure> {
        match frame.sub {
            JsSub::Comment => {
                if self.until(start, b"*/")? {
                    frame.sub = JsSub::Code;
                    self.set_top(Frame::Js(frame));
                }
                return Ok(self.lexeme(Some(TokenClass::Comment), start));
            }
            JsSub::Str(quote) => {
                let end = self.string_body(start, quote)?;
                self.string_ended(frame, quote, end);
                return Ok(self.lexeme(Some(TokenClass::String), start));
            }
            JsSub::Code => {}
        }
        if is_blank(byte) {
            return self.blank(start);
        }
        let next = self.peek_at(1)?.unwrap_or(0);
        let class = match byte {
            b'/' if next == b'/' => {
                self.scratch.rest = Some(TokenClass::Comment);
                return self.rest_of_line(TokenClass::Comment, start);
            }
            b'#' if next == b'!' && self.line == 0 && start == 0 => {
                return self.rest_of_line(TokenClass::Comment, start);
            }
            b'/' if next == b'*' => {
                self.take_n(2)?;
                frame.sub = JsSub::Comment;
                if self.until(start, b"*/")? {
                    frame.sub = JsSub::Code;
                }
                self.set_top(Frame::Js(frame));
                return Ok(self.lexeme(Some(TokenClass::Comment), start));
            }
            b'"' | b'\'' => {
                self.take()?;
                let end = self.string_body(start, byte)?;
                frame.regex_ok = false;
                self.string_ended(frame, byte, end);
                return Ok(self.lexeme(Some(TokenClass::String), start));
            }
            b'`' => {
                frame.regex_ok = false;
                self.set_top(Frame::Js(frame));
                return self.opened(Frame::Template(frame.dialect), 1, TokenClass::String, start);
            }
            b'/' if frame.regex_ok => {
                self.regex(start)?;
                frame.regex_ok = false;
                TokenClass::String
            }
            b'{' => {
                self.take()?;
                if frame.end == End::Brace {
                    frame.braces = frame.braces.saturating_add(1);
                }
                frame.regex_ok = true;
                TokenClass::Punctuation
            }
            b'}' => {
                self.take()?;
                if frame.end == End::Brace && frame.braces == 0 {
                    self.close();
                    return Ok(self.lexeme(Some(TokenClass::Punctuation), start));
                }
                frame.braces = frame.braces.saturating_sub(1);
                frame.regex_ok = true;
                TokenClass::Punctuation
            }
            b'<' => return self.js_angle(frame, next, start),
            b'>' if self.in_generic() => {
                self.take()?;
                frame.regex_ok = false;
                TokenClass::Punctuation
            }
            _ if byte.is_ascii_digit() || (byte == b'.' && next.is_ascii_digit()) => {
                self.number(start, false)?;
                frame.regex_ok = false;
                TokenClass::Number
            }
            b'.' if next != b'.' => {
                self.take()?;
                self.scratch.js.after_dot = true;
                frame.regex_ok = false;
                TokenClass::Punctuation
            }
            b'#' | b'@' if is_ident_start(next) => {
                self.take()?;
                self.take_word(start, b"")?;
                frame.regex_ok = false;
                self.set_top(Frame::Js(frame));
                let class = (byte == b'@').then_some(TokenClass::Function);
                return Ok(self.lexeme(class, start));
            }
            _ if is_ident_start(byte) => return self.js_word(frame, start),
            b'(' | b'[' | b';' | b',' => {
                self.take()?;
                frame.regex_ok = true;
                TokenClass::Punctuation
            }
            b')' | b']' => {
                self.take()?;
                frame.regex_ok = false;
                TokenClass::Punctuation
            }
            b':' => {
                self.take()?;
                frame.regex_ok = true;
                if self.scratch.js.ternary > 0 {
                    self.scratch.js.ternary -= 1;
                    TokenClass::Operator
                } else {
                    self.scratch.js.annot = frame.dialect.ts();
                    TokenClass::Punctuation
                }
            }
            _ if b"=!<>&|+-*/%^~?.".contains(&byte) => {
                self.js_operator(&mut frame)?;
                TokenClass::Operator
            }
            _ => {
                self.take()?;
                TokenClass::Invalid
            }
        };
        self.set_top(Frame::Js(frame));
        Ok(self.lexeme(Some(class), start))
    }

    /// Opens `frame` and takes its opener of `width` characters as `class`.
    pub(super) fn opened(
        &mut self,
        frame: Frame,
        width: usize,
        class: TokenClass,
        start: u32,
    ) -> Result<Lexeme, Failure> {
        if let Some(lexeme) = self.open(frame, start)? {
            return Ok(lexeme);
        }
        self.take_n(width)?;
        Ok(self.lexeme(Some(class), start))
    }

    /// A string's piece ended so: it goes on on the next line, or in the next piece, or not.
    fn string_ended(&mut self, mut frame: JsFrame, quote: u8, end: StrEnd) {
        frame.sub = match end {
            StrEnd::Escaped | StrEnd::Cut => JsSub::Str(quote),
            StrEnd::Closed | StrEnd::LineEnd => JsSub::Code,
        };
        self.set_top(Frame::Js(frame));
    }

    /// Whether the next byte is inside type arguments.
    pub(super) fn in_generic(&self) -> bool {
        self.reader.offset() < self.scratch.js.generic_end
    }

    /// `<`: a JSX element where a value may start, type arguments in TypeScript, or an operator.
    fn js_angle(&mut self, mut frame: JsFrame, next: u8, start: u32) -> Result<Lexeme, Failure> {
        let element = next.is_ascii_alphabetic() || matches!(next, b'>' | b'_');
        if frame.dialect.jsx() && frame.regex_ok && element {
            frame.regex_ok = false;
            self.set_top(Frame::Js(frame));
            let tag = Frame::JsxTag(frame.dialect, JsxSub::Name);
            return self.opened(tag, 1, TokenClass::Punctuation, start);
        }
        if frame.dialect.ts() && !self.in_generic() {
            let len =
                super::js_words::generic_len(self.reader.look(super::js_words::GENERIC_LOOK)?);
            if len > 0 {
                self.scratch.js.generic_end = self.reader.offset() + len as u64;
            }
        }
        let class = if self.in_generic() {
            self.take()?;
            TokenClass::Punctuation
        } else {
            self.js_operator(&mut frame)?;
            TokenClass::Operator
        };
        self.set_top(Frame::Js(frame));
        Ok(self.lexeme(Some(class), start))
    }

    /// An operator, the longest that matches.
    fn js_operator(&mut self, frame: &mut JsFrame) -> Result<(), Failure> {
        let bytes = self.reader.look(4)?;
        let len = OPERATORS
            .iter()
            .find(|operator| bytes.starts_with(operator))
            .map_or(1, |operator| operator.len());
        let first = bytes.first().copied().unwrap_or(0);
        let second = bytes.get(1).copied().unwrap_or(0);
        let ts = frame.dialect.ts();
        match (len, first) {
            (2, b'+' | b'-') if second == first => {}
            (2, b'?') if second == b'.' => {
                self.scratch.js.after_dot = true;
                frame.regex_ok = false;
            }
            (1, b'?') => {
                // `x?: T` and `x?)` mark an optional name in TypeScript; else a condition.
                if !(ts && matches!(second, b':' | b')' | b',')) {
                    self.scratch.js.ternary = self.scratch.js.ternary.saturating_add(1);
                }
                frame.regex_ok = true;
            }
            (1, b'|' | b'&') => {
                self.scratch.js.annot = ts && self.scratch.js.in_type;
                frame.regex_ok = true;
            }
            _ => frame.regex_ok = true,
        }
        self.take_n(len)
    }

    /// A regular expression from its `/`: classes `[...]` and escapes hold a `/`, the flags
    /// follow it.
    fn regex(&mut self, start: u32) -> Result<(), Failure> {
        self.take()?;
        let mut class = false;
        while let Some(byte) = self.more(start)? {
            self.take()?;
            match byte {
                b'\\' => {
                    if self.more(start)?.is_some() {
                        self.take()?;
                    }
                }
                b'[' => class = true,
                b']' => class = false,
                b'/' if !class => {
                    while let Some(flag) = self.more(start)?
                        && is_ident(flag)
                    {
                        self.take()?;
                    }
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "js_tests.rs"]
mod tests;
