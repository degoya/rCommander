//! HTML, at the bottom of an HTML, Vue or PHP file: text, entities, comments, `<!DOCTYPE>`, tags
//! and attributes. A `<script>` or `<style>` start tag opens JavaScript or CSS for the element's
//! content, a `style` or `on...` attribute CSS or JavaScript for its value; `lang` and `type`
//! choose the dialect (`<script lang="ts">`, `<style lang="scss">`).
//!
//! Classes: element names `type-name`, attribute names `key`, values `string`, entities
//! `literal`, `<` `</` `>` `/>` `punctuation`, `=` `operator`, comments `comment`, a
//! declaration `keyword`; text stays uncoloured.

use super::scan::{Word, is_blank};
use super::state::{Attr, CssDialect, CssFrame, Dialect, Elem, End, Frame, HtmlSub, JsFrame};
use super::{Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

/// The bytes of a `lang` or `type` value kept to choose a dialect.
const VALUE_BYTES: usize = 24;

/// What one line of HTML keeps for itself.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Scratch {
    /// The `lang` or `type` value read so far, in lower case.
    value: [u8; VALUE_BYTES],
    len: usize,
}

impl Scratch {
    fn push(&mut self, byte: u8) {
        if let Some(slot) = self.value.get_mut(self.len) {
            *slot = byte.to_ascii_lowercase();
            self.len += 1;
        }
    }

    fn value(&self) -> &[u8] {
        self.value.get(..self.len).unwrap_or_default()
    }
}

/// Whether a byte ends an attribute's name.
fn ends_name(byte: u8) -> bool {
    is_blank(byte) || matches!(byte, b'"' | b'\'' | b'>' | b'/' | b'=' | b'<')
}

/// The element a start tag named `word` opens.
fn element(word: &Word) -> Elem {
    if word.is_in_any_case(&["script"]) {
        Elem::Script(Dialect::Js)
    } else if word.is_in_any_case(&["style"]) {
        Elem::Style(CssDialect::Css)
    } else {
        Elem::Other
    }
}

/// What an attribute named `word` holds.
fn attribute(word: &Word) -> Attr {
    let text = word.text();
    if word.is_in_any_case(&["style"]) {
        Attr::Style
    } else if super::words::is_event(text) {
        Attr::Event
    } else if word.is_in_any_case(&["lang"]) {
        Attr::Lang
    } else if word.is_in_any_case(&["type"]) {
        Attr::Type
    } else {
        Attr::Plain
    }
}

/// Whether `needle` is in `haystack`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The element after its `lang` or `type` attribute said `value`.
fn retype(elem: Elem, attr: Attr, value: &[u8]) -> Elem {
    match (elem, attr) {
        (Elem::Script(_) | Elem::RawScript, Attr::Lang) => match value {
            b"ts" | b"typescript" => Elem::Script(Dialect::Ts),
            b"tsx" => Elem::Script(Dialect::Tsx),
            b"jsx" => Elem::Script(Dialect::Jsx),
            _ => Elem::Script(Dialect::Js),
        },
        (Elem::Script(_) | Elem::RawScript, Attr::Type) => {
            if contains(value, b"typescript") {
                Elem::Script(Dialect::Ts)
            } else if contains(value, b"babel") || contains(value, b"jsx") {
                Elem::Script(Dialect::Jsx)
            } else if value.is_empty()
                || value == b"module"
                || contains(value, b"javascript")
                || contains(value, b"ecmascript")
                || contains(value, b"json")
            {
                Elem::Script(Dialect::Js)
            } else {
                Elem::RawScript
            }
        }
        (Elem::Style(_), Attr::Lang) => match value {
            b"scss" | b"sass" => Elem::Style(CssDialect::Scss),
            b"less" => Elem::Style(CssDialect::Less),
            _ => Elem::Style(CssDialect::Css),
        },
        _ => elem,
    }
}

impl<S: Source> Lexer<'_, S> {
    pub(super) fn html(&mut self, sub: HtmlSub, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        match sub {
            HtmlSub::Text => self.html_text(byte, start),
            HtmlSub::Comment => self.html_piece(start, b"-->", TokenClass::Comment),
            HtmlSub::Decl => self.html_piece(start, b">", TokenClass::Keyword),
            HtmlSub::TagName => self.tag_name(start, false),
            HtmlSub::EndTagName => self.tag_name(start, true),
            HtmlSub::Tag(elem, attr) => self.in_tag(elem, attr, byte, start),
            HtmlSub::Eq(elem, attr) => self.attr_value(elem, attr, byte, start),
            HtmlSub::Value(elem, attr, quote) => self.value_body(elem, attr, quote, start),
            HtmlSub::Embedded(elem, quote) => {
                self.set_top(Frame::Html(HtmlSub::Tag(elem, Attr::Plain)));
                if byte != quote {
                    return self.in_tag(elem, Attr::Plain, byte, start);
                }
                self.take()?;
                Ok(self.lexeme(Some(TokenClass::String), start))
            }
            HtmlSub::EndTag => {
                if is_blank(byte) {
                    return self.blank(start);
                }
                self.take()?;
                if byte != b'>' {
                    return Ok(self.lexeme(None, start));
                }
                self.set_top(Frame::Html(HtmlSub::Text));
                Ok(self.lexeme(Some(TokenClass::Punctuation), start))
            }
        }
    }

    /// Text, an entity, or the start of markup.
    fn html_text(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        if byte == b'<' {
            let next = self.peek_at(1)?.unwrap_or(0);
            if next == b'!' {
                let comment = self.reader.look(4)? == b"<!--";
                let (open, sub, closer, class): (usize, HtmlSub, &[u8], TokenClass) = if comment {
                    (4, HtmlSub::Comment, &b"-->"[..], TokenClass::Comment)
                } else {
                    (2, HtmlSub::Decl, &b">"[..], TokenClass::Keyword)
                };
                self.take_n(open)?;
                self.set_top(Frame::Html(sub));
                return self.html_piece(start, closer, class);
            }
            if next == b'/'
                && self
                    .peek_at(2)?
                    .is_some_and(|byte| byte.is_ascii_alphabetic())
            {
                self.take_n(2)?;
                self.set_top(Frame::Html(HtmlSub::EndTagName));
                return Ok(self.lexeme(Some(TokenClass::Punctuation), start));
            }
            if next.is_ascii_alphabetic() {
                self.take()?;
                self.set_top(Frame::Html(HtmlSub::TagName));
                return Ok(self.lexeme(Some(TokenClass::Punctuation), start));
            }
        }
        if byte == b'&'
            && let Some(lexeme) = self.entity(start)?
        {
            return Ok(lexeme);
        }
        self.take()?;
        while let Some(byte) = self.more(start)?
            && !matches!(byte, b'<' | b'&')
        {
            self.take()?;
        }
        Ok(self.lexeme(None, start))
    }

    /// A comment's or a declaration's piece up to its `closer`; text follows the closer.
    fn html_piece(
        &mut self,
        start: u32,
        closer: &[u8],
        class: TokenClass,
    ) -> Result<Lexeme, Failure> {
        if self.until(start, closer)? {
            self.set_top(Frame::Html(HtmlSub::Text));
        }
        Ok(self.lexeme(Some(class), start))
    }

    /// An element's name after `<` or `</`.
    fn tag_name(&mut self, start: u32, end: bool) -> Result<Lexeme, Failure> {
        let word = self.word_ahead(b"-:.")?;
        self.take_word(start, b"-:.")?;
        let sub = if end {
            HtmlSub::EndTag
        } else {
            HtmlSub::Tag(element(&word), Attr::Plain)
        };
        self.set_top(Frame::Html(sub));
        Ok(self.lexeme(Some(TokenClass::TypeName), start))
    }

    /// In a start tag: blanks, an attribute's name, `=`, `>` or `/>`.
    fn in_tag(&mut self, elem: Elem, attr: Attr, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        if is_blank(byte) {
            return self.blank(start);
        }
        match byte {
            b'>' => {
                self.take()?;
                self.set_top(Frame::Html(HtmlSub::Text));
                let content = match elem {
                    Elem::Script(dialect) => Frame::Js(JsFrame::new(dialect, End::ScriptTag)),
                    Elem::Style(dialect) => Frame::Css(CssFrame::new(dialect, End::StyleTag, 0)),
                    Elem::RawScript => Frame::Raw,
                    Elem::Other => return Ok(self.lexeme(Some(TokenClass::Punctuation), start)),
                };
                if let Some(lexeme) = self.open(content, start)? {
                    return Ok(lexeme);
                }
                Ok(self.lexeme(Some(TokenClass::Punctuation), start))
            }
            b'/' if self.peek_at(1)? == Some(b'>') => {
                self.take_n(2)?;
                self.set_top(Frame::Html(HtmlSub::Text));
                Ok(self.lexeme(Some(TokenClass::Punctuation), start))
            }
            b'=' => {
                self.take()?;
                self.set_top(Frame::Html(HtmlSub::Eq(elem, attr)));
                Ok(self.lexeme(Some(TokenClass::Operator), start))
            }
            b'"' | b'\'' => self.attr_value(elem, Attr::Plain, byte, start),
            _ => {
                let word = self.word_ahead(b"-:.@#")?;
                self.take()?;
                while let Some(byte) = self.more(start)?
                    && !ends_name(byte)
                {
                    self.take()?;
                }
                self.set_top(Frame::Html(HtmlSub::Tag(elem, attribute(&word))));
                Ok(self.lexeme(Some(TokenClass::Key), start))
            }
        }
    }

    /// After `=`: a quoted or bare value, CSS or JavaScript for `style` and `on...`.
    fn attr_value(
        &mut self,
        elem: Elem,
        attr: Attr,
        byte: u8,
        start: u32,
    ) -> Result<Lexeme, Failure> {
        if is_blank(byte) {
            return self.blank(start);
        }
        self.scratch.html = Scratch::default();
        match byte {
            b'"' | b'\'' => {
                self.take()?;
                let embedded = match attr {
                    Attr::Style => Some(Frame::Css(CssFrame::new(
                        CssDialect::Css,
                        End::Quote(byte),
                        1,
                    ))),
                    Attr::Event => Some(Frame::Js(JsFrame::new(Dialect::Js, End::Quote(byte)))),
                    _ => None,
                };
                let Some(frame) = embedded else {
                    self.set_top(Frame::Html(HtmlSub::Value(elem, attr, byte)));
                    return self.value_body(elem, attr, byte, start);
                };
                self.set_top(Frame::Html(HtmlSub::Embedded(elem, byte)));
                if let Some(lexeme) = self.open(frame, start)? {
                    return Ok(lexeme);
                }
                Ok(self.lexeme(Some(TokenClass::String), start))
            }
            b'>' => {
                self.set_top(Frame::Html(HtmlSub::Tag(elem, Attr::Plain)));
                self.in_tag(elem, Attr::Plain, byte, start)
            }
            _ => {
                self.scratch.html.push(byte);
                self.take()?;
                while let Some(byte) = self.more(start)?
                    && !is_blank(byte)
                    && byte != b'>'
                {
                    self.scratch.html.push(byte);
                    self.take()?;
                }
                let elem = retype(elem, attr, self.scratch.html.value());
                self.set_top(Frame::Html(HtmlSub::Tag(elem, Attr::Plain)));
                Ok(self.lexeme(Some(TokenClass::String), start))
            }
        }
    }

    /// A quoted value's piece up to its closing `quote`; an entity in it is a piece of its own.
    fn value_body(
        &mut self,
        elem: Elem,
        attr: Attr,
        quote: u8,
        start: u32,
    ) -> Result<Lexeme, Failure> {
        while let Some(byte) = self.more(start)? {
            if byte == b'&' {
                if self.col > start {
                    break;
                }
                if let Some(lexeme) = self.entity(start)? {
                    return Ok(lexeme);
                }
            }
            self.take()?;
            if byte == quote {
                let elem = retype(elem, attr, self.scratch.html.value());
                self.set_top(Frame::Html(HtmlSub::Tag(elem, Attr::Plain)));
                break;
            }
            self.scratch.html.push(byte);
        }
        Ok(self.lexeme(Some(TokenClass::String), start))
    }
}

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
