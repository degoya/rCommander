//! CSS, and SCSS and LESS read as CSS with nesting, variables and `//` comments: in a `.css`,
//! `.scss` or `.less` file, a `<style>` element or a `style` attribute.
//!
//! Whether a line's next statement is a selector or a declaration is decided where it starts,
//! by what comes first on the line after it: `{` makes a selector, `;` or `}` a declaration;
//! neither, a line ending in `,` is a selector, else a declaration inside a block and a selector
//! outside. That holds for nested rules too (`&:hover {` against `color: red;`).
//!
//! Classes: selectors (elements, `.class`, `#id`, `:hover`, `&`) `type-name`; properties,
//! `$x` and `@x` variables, media features `key`; at-rules `keyword`; numbers, measures and
//! `#fff` colours `number`; `!important` and keyword values (`auto`) `literal`; strings and
//! `url(...)` `string`; `calc(` and the other functions `function`.

use super::scan::{is_blank, is_ident, is_ident_start};
use super::state::{CssDialect, CssFrame, End, Frame};
use super::words::{CSS_AT_NAMES, CSS_VALUES, LESS_AT_RULES};
use super::{Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

/// The bytes looked at to decide a statement.
const STATEMENT_LOOK: usize = 1024;

/// What the statement on a line is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stmt {
    /// Not decided yet.
    #[default]
    Unknown,
    Selector,
    Decl,
}

/// What one line of CSS keeps for itself.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Scratch {
    stmt: Stmt,
    /// After an at-rule, up to its `{` or `;`: read as a value.
    prelude: bool,
    /// The SCSS `#{` and LESS `@{` interpolations open.
    interp: u8,
    /// In a selector's `[...]`.
    attr: bool,
    /// After `@include` or `@mixin`: the next word names a mixin.
    name_next: bool,
    /// After `url`: its `(` comes; then its unquoted address.
    url: bool,
    url_body: bool,
}

impl<S: Source> Lexer<'_, S> {
    pub(super) fn css(
        &mut self,
        mut frame: CssFrame,
        byte: u8,
        start: u32,
    ) -> Result<Lexeme, Failure> {
        if frame.comment {
            if self.until(start, b"*/")? {
                frame.comment = false;
                self.set_top(Frame::Css(frame));
            }
            return Ok(self.lexeme(Some(TokenClass::Comment), start));
        }
        if self.scratch.css.url_body && !matches!(byte, b'"' | b'\'' | b')') && !is_blank(byte) {
            self.scratch.css.url_body = false;
            self.take()?;
            while let Some(byte) = self.more(start)?
                && byte != b')'
            {
                self.take()?;
            }
            return Ok(self.lexeme(Some(TokenClass::String), start));
        }
        self.scratch.css.url_body = false;
        if is_blank(byte) {
            return self.blank(start);
        }
        let next = self.peek_at(1)?.unwrap_or(0);
        match byte {
            b'/' if next == b'*' => {
                self.take_n(2)?;
                frame.comment = true;
                if self.until(start, b"*/")? {
                    frame.comment = false;
                }
                self.set_top(Frame::Css(frame));
                return Ok(self.lexeme(Some(TokenClass::Comment), start));
            }
            b'/' if next == b'/' && frame.dialect != CssDialect::Css => {
                self.scratch.rest = Some(TokenClass::Comment);
                return self.rest_of_line(TokenClass::Comment, start);
            }
            b'"' | b'\'' => {
                self.take()?;
                self.string_body(start, byte)?;
                return Ok(self.lexeme(Some(TokenClass::String), start));
            }
            b'#' | b'@' if next == b'{' && frame.dialect != CssDialect::Css => {
                self.take_n(2)?;
                self.scratch.css.interp = self.scratch.css.interp.saturating_add(1);
                return self.punctuation(start);
            }
            b'{' => {
                self.take()?;
                frame.depth = frame.depth.saturating_add(1);
                return self.css_reset(frame, start);
            }
            b'}' => {
                self.take()?;
                if self.scratch.css.interp > 0 {
                    self.scratch.css.interp -= 1;
                    return self.punctuation(start);
                }
                frame.depth = frame.depth.saturating_sub(1);
                return self.css_reset(frame, start);
            }
            b';' => {
                self.take()?;
                return self.css_reset(frame, start);
            }
            b'@' => return self.at_word(frame, start),
            b'!' if is_ident_start(next) => {
                self.take()?;
                self.take_word(start, b"-")?;
                return Ok(self.lexeme(Some(TokenClass::Literal), start));
            }
            b'$' if frame.dialect == CssDialect::Scss && is_ident_start(next) => {
                self.take_word(start, b"-")?;
                return Ok(self.lexeme(Some(TokenClass::Key), start));
            }
            _ => {}
        }
        if frame.value || self.scratch.css.prelude || self.scratch.css.interp > 0 {
            return self.css_value(byte, next, start);
        }
        if self.scratch.css.stmt == Stmt::Unknown {
            self.scratch.css.stmt = self.statement(frame)?;
        }
        if self.scratch.css.stmt == Stmt::Selector {
            return self.selector(byte, next, start);
        }
        match byte {
            b':' => {
                self.take()?;
                frame.value = true;
                self.set_top(Frame::Css(frame));
                self.punctuation(start)
            }
            b'.' | b'#' if is_ident_start(next) || next == b'-' => {
                self.take()?;
                self.take_word(start, b"-")?;
                Ok(self.lexeme(Some(TokenClass::Function), start))
            }
            b'&' => self.selector(byte, next, start),
            _ if is_ident_start(byte) || byte == b'-' => {
                self.take_word(start, b"-")?;
                Ok(self.lexeme(Some(TokenClass::Key), start))
            }
            _ => self.css_value(byte, next, start),
        }
    }

    /// After `{`, `}` or `;`: a new statement starts.
    fn css_reset(&mut self, mut frame: CssFrame, start: u32) -> Result<Lexeme, Failure> {
        frame.value = false;
        self.set_top(Frame::Css(frame));
        self.scratch.css.stmt = Stmt::Unknown;
        self.scratch.css.prelude = false;
        self.scratch.css.attr = false;
        self.punctuation(start)
    }

    fn punctuation(&mut self, start: u32) -> Result<Lexeme, Failure> {
        Ok(self.lexeme(Some(TokenClass::Punctuation), start))
    }

    /// `@` and its name: an at-rule, or a LESS variable.
    fn at_word(&mut self, frame: CssFrame, start: u32) -> Result<Lexeme, Failure> {
        self.take()?;
        let word = self.word_ahead(b"-")?;
        if self
            .more(start)?
            .is_some_and(|byte| is_ident(byte) || byte == b'-')
        {
            self.take_word(start, b"-")?;
        }
        if frame.dialect == CssDialect::Less && !word.is_in_any_case(LESS_AT_RULES) {
            return Ok(self.lexeme(Some(TokenClass::Key), start));
        }
        if !frame.value {
            self.scratch.css.prelude = true;
        }
        self.scratch.css.name_next = word.is_in(CSS_AT_NAMES);
        Ok(self.lexeme(Some(TokenClass::Keyword), start))
    }

    /// Whether the statement starting here is a selector or a declaration.
    fn statement(&mut self, frame: CssFrame) -> Result<Stmt, Failure> {
        let quote = match frame.end {
            End::Quote(quote) => quote,
            _ => b'\n',
        };
        let bytes = self.reader.look(STATEMENT_LOOK)?;
        let (mut prev, mut last, mut interp) = (0u8, 0u8, false);
        for &byte in bytes {
            match byte {
                b'\n' => break,
                _ if byte == quote => break,
                b'{' if matches!(prev, b'#' | b'@') => interp = true,
                b'}' if interp => interp = false,
                b'{' if !interp => return Ok(Stmt::Selector),
                b';' | b'}' if !interp => return Ok(Stmt::Decl),
                _ => {}
            }
            prev = byte;
            if !is_blank(byte) {
                last = byte;
            }
        }
        Ok(if last == b',' || frame.depth == 0 {
            Stmt::Selector
        } else {
            Stmt::Decl
        })
    }

    /// A selector's part.
    fn selector(&mut self, byte: u8, next: u8, start: u32) -> Result<Lexeme, Failure> {
        let class = match byte {
            b'.' | b'#' | b'%' if is_ident_start(next) || next == b'-' => {
                self.take()?;
                self.take_word(start, b"-")?;
                TokenClass::TypeName
            }
            b'&' | b'*' => {
                self.take()?;
                while let Some(byte) = self.more(start)?
                    && (is_ident(byte) || byte == b'-')
                {
                    self.take()?;
                }
                TokenClass::TypeName
            }
            b':' => {
                self.take()?;
                if self.more(start)? == Some(b':') {
                    self.take()?;
                }
                if self
                    .more(start)?
                    .is_some_and(|byte| is_ident_start(byte) || byte == b'-')
                {
                    self.take_word(start, b"-")?;
                }
                TokenClass::TypeName
            }
            b'[' | b']' => {
                self.take()?;
                self.scratch.css.attr = byte == b'[';
                TokenClass::Punctuation
            }
            b'(' | b')' | b',' => {
                self.take()?;
                TokenClass::Punctuation
            }
            b'>' | b'+' | b'~' | b'=' | b'^' | b'|' => {
                self.take()?;
                if self.more(start)? == Some(b'=') {
                    self.take()?;
                }
                TokenClass::Operator
            }
            _ if byte.is_ascii_digit() => return self.number(start, true),
            _ if is_ident_start(byte) || byte == b'-' => {
                self.take_word(start, b"-")?;
                if self.scratch.css.attr {
                    TokenClass::Key
                } else {
                    TokenClass::TypeName
                }
            }
            _ => {
                self.take()?;
                return Ok(self.lexeme(None, start));
            }
        };
        Ok(self.lexeme(Some(class), start))
    }

    /// A part of a value, an at-rule's prelude or an interpolation.
    fn css_value(&mut self, byte: u8, next: u8, start: u32) -> Result<Lexeme, Failure> {
        let signed = matches!(byte, b'-' | b'+' | b'.') && next.is_ascii_digit();
        if byte.is_ascii_digit() || signed {
            return self.number(start, true);
        }
        if byte == b'#' && is_ident(next) {
            self.take()?;
            self.take_word(start, b"")?;
            return Ok(self.lexeme(Some(TokenClass::Number), start));
        }
        if is_ident_start(byte) || (byte == b'-' && (is_ident_start(next) || next == b'-')) {
            let word = self.word_ahead(b"-")?;
            self.take_word(start, b"-")?;
            let after = self.after_blanks()?;
            let class = if std::mem::take(&mut self.scratch.css.name_next) {
                Some(TokenClass::Function)
            } else if self.peek()? == Some(b'(') {
                // A CSS function's name touches its parenthesis; `and (` in a media query is no call.
                self.scratch.css.url = word.is_in_any_case(&["url"]);
                Some(TokenClass::Function)
            } else if self.scratch.css.prelude && after[0] == b':' {
                Some(TokenClass::Key)
            } else if word.is_in_any_case(CSS_VALUES) {
                Some(TokenClass::Literal)
            } else {
                None
            };
            return Ok(self.lexeme(class, start));
        }
        self.take()?;
        let class = match byte {
            b'(' => {
                self.scratch.css.url_body = std::mem::take(&mut self.scratch.css.url);
                TokenClass::Punctuation
            }
            b')' | b',' | b':' | b'[' | b']' => TokenClass::Punctuation,
            b'+' | b'-' | b'*' | b'/' | b'=' | b'<' | b'>' | b'~' | b'%' | b'&' | b'|' | b'^'
            | b'?' => TokenClass::Operator,
            _ => return Ok(self.lexeme(None, start)),
        };
        Ok(self.lexeme(Some(class), start))
    }
}

#[cfg(test)]
#[path = "css_tests.rs"]
mod tests;
