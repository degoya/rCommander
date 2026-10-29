//! The frames JavaScript opens inside itself: a template string, whose `${` opens JavaScript
//! again, and JSX elements in `.jsx` and `.tsx` - a start tag, the children, an end tag - whose
//! `{` opens JavaScript again.
//!
//! Classes: template text and JSX attribute strings `string`; `${`, `{`, `}`, `<`, `</`, `>`,
//! `/>` `punctuation`; element names `type-name`; attribute names `key`; `=` `operator`;
//! entities `literal`; children's text stays uncoloured.

use super::scan::{is_blank, is_ident_start};
use super::state::{Dialect, End, Frame, JsFrame, JsxSub};
use super::{Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

impl<S: Source> Lexer<'_, S> {
    /// A template string's text up to its `` ` `` or the next `${`.
    pub(super) fn template(&mut self, dialect: Dialect, start: u32) -> Result<Lexeme, Failure> {
        let first = self.peek()?.unwrap_or(0);
        if first == b'`' {
            self.take()?;
            self.close();
            return Ok(self.lexeme(Some(TokenClass::String), start));
        }
        if first == b'$' && self.peek_at(1)? == Some(b'{') {
            let code = Frame::Js(JsFrame::new(dialect, End::Brace));
            return self.opened(code, 2, TokenClass::Punctuation, start);
        }
        while let Some(byte) = self.more(start)? {
            if byte == b'`' || (byte == b'$' && self.peek_at(1)? == Some(b'{')) {
                break;
            }
            self.take()?;
            if byte == b'\\' && self.more(start)?.is_some() {
                self.take()?;
            }
        }
        Ok(self.lexeme(Some(TokenClass::String), start))
    }

    /// In a JSX start tag.
    pub(super) fn jsx_tag(
        &mut self,
        dialect: Dialect,
        sub: JsxSub,
        byte: u8,
        start: u32,
    ) -> Result<Lexeme, Failure> {
        match sub {
            JsxSub::Name if is_ident_start(byte) => {
                self.take_word(start, b"-:.")?;
                self.set_top(Frame::JsxTag(dialect, JsxSub::Attrs));
                Ok(self.lexeme(Some(TokenClass::TypeName), start))
            }
            JsxSub::Name | JsxSub::Attrs => self.jsx_attrs(dialect, byte, start),
            JsxSub::Eq => match byte {
                b'"' | b'\'' => {
                    self.take()?;
                    self.jsx_string(dialect, byte, start)
                }
                b'{' => {
                    self.set_top(Frame::JsxTag(dialect, JsxSub::Attrs));
                    self.jsx_code(start)
                }
                _ if is_blank(byte) => self.blank(start),
                _ => self.jsx_attrs(dialect, byte, start),
            },
            JsxSub::Str(quote) => self.jsx_string(dialect, quote, start),
        }
    }

    /// At a JSX start tag's attributes, `>` or `/>`.
    fn jsx_attrs(&mut self, dialect: Dialect, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        if is_blank(byte) {
            return self.blank(start);
        }
        self.set_top(Frame::JsxTag(dialect, JsxSub::Attrs));
        let class = match byte {
            b'/' if self.peek_at(1)? == Some(b'>') => {
                self.take_n(2)?;
                self.close();
                TokenClass::Punctuation
            }
            b'>' => {
                self.take()?;
                self.set_top(Frame::JsxChildren(dialect));
                TokenClass::Punctuation
            }
            b'{' => return self.jsx_code(start),
            b'=' => {
                self.take()?;
                self.set_top(Frame::JsxTag(dialect, JsxSub::Eq));
                TokenClass::Operator
            }
            _ if is_ident_start(byte) => {
                self.take_word(start, b"-:")?;
                TokenClass::Key
            }
            _ => {
                self.take()?;
                TokenClass::Invalid
            }
        };
        Ok(self.lexeme(Some(class), start))
    }

    /// An attribute string's piece up to its closing `quote`; JSX knows no escapes in it.
    fn jsx_string(&mut self, dialect: Dialect, quote: u8, start: u32) -> Result<Lexeme, Failure> {
        let mut sub = JsxSub::Str(quote);
        while let Some(byte) = self.more(start)? {
            self.take()?;
            if byte == quote {
                sub = JsxSub::Attrs;
                break;
            }
        }
        self.set_top(Frame::JsxTag(dialect, sub));
        Ok(self.lexeme(Some(TokenClass::String), start))
    }

    /// `{`: JavaScript up to its `}`.
    fn jsx_code(&mut self, start: u32) -> Result<Lexeme, Failure> {
        let dialect = match self.state.stack.top() {
            Frame::JsxTag(dialect, _) | Frame::JsxChildren(dialect) => dialect,
            _ => Dialect::Jsx,
        };
        let code = Frame::Js(JsFrame::new(dialect, End::Brace));
        self.opened(code, 1, TokenClass::Punctuation, start)
    }

    /// A JSX element's children: text, entities, `{...}`, elements and the end tag's `</`.
    pub(super) fn jsx_children(
        &mut self,
        dialect: Dialect,
        byte: u8,
        start: u32,
    ) -> Result<Lexeme, Failure> {
        let next = self.peek_at(1)?.unwrap_or(0);
        match byte {
            b'{' => return self.jsx_code(start),
            b'<' if next == b'/' => {
                self.take_n(2)?;
                self.set_top(Frame::JsxClose);
                return Ok(self.lexeme(Some(TokenClass::Punctuation), start));
            }
            b'<' if next.is_ascii_alphabetic() || matches!(next, b'>' | b'_') => {
                let tag = Frame::JsxTag(dialect, JsxSub::Name);
                return self.opened(tag, 1, TokenClass::Punctuation, start);
            }
            b'&' => {
                if let Some(lexeme) = self.entity(start)? {
                    return Ok(lexeme);
                }
            }
            _ => {}
        }
        self.take()?;
        while let Some(byte) = self.more(start)?
            && !matches!(byte, b'{' | b'<' | b'&')
        {
            self.take()?;
        }
        Ok(self.lexeme(None, start))
    }

    /// A JSX end tag after `</`: its name and `>`.
    pub(super) fn jsx_close(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        if is_blank(byte) {
            return self.blank(start);
        }
        let class = if byte == b'>' {
            self.take()?;
            self.close();
            TokenClass::Punctuation
        } else if is_ident_start(byte) {
            self.take_word(start, b"-:.")?;
            TokenClass::TypeName
        } else {
            self.take()?;
            TokenClass::Invalid
        };
        Ok(self.lexeme(Some(class), start))
    }
}

#[cfg(test)]
#[path = "jsx_tests.rs"]
mod tests;
