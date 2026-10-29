//! The lexer's scanners: one lexeme from the byte after the whitespace, in the mode the lexer is
//! in (`super::Lexer::next_with` picks the scanner). Each takes at least one byte and stops before
//! a line break, so no lexeme spans a line.

use super::{Kind, Lexer, Mode, Problem, is_name_start};
use crate::reader::Bytes;
use crate::types::Failure;

fn is_name_char(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
}

fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

impl<B: Bytes> Lexer<B> {
    pub(super) fn content(&mut self, first: u8) -> Result<Kind, Failure> {
        match first {
            b'<' => self.markup(),
            b'&' => self.entity(),
            _ => {
                while let Some(byte) = self.bytes.peek()? {
                    if matches!(byte, b'<' | b'&' | b'\r' | b'\n') {
                        break;
                    }
                    self.take(byte);
                }
                Ok(Kind::Text)
            }
        }
    }

    /// After `<`: a tag, an end tag, a comment, a CDATA section, a declaration or a PI.
    fn markup(&mut self) -> Result<Kind, Failure> {
        self.take(b'<');
        match self.bytes.peek()? {
            Some(byte) if is_name_start(byte) => {
                self.mode = Mode::Name { end: false };
                Ok(Kind::Open)
            }
            Some(b'/') => {
                self.take(b'/');
                match self.bytes.peek()? {
                    Some(byte) if is_name_start(byte) => {
                        self.mode = Mode::Name { end: true };
                        Ok(Kind::EndOpen)
                    }
                    _ => Ok(Kind::Invalid(Problem::BadMarkup)),
                }
            }
            Some(b'?') => {
                self.take(b'?');
                self.mode = Mode::Pi;
                self.pi()
            }
            Some(b'!') => {
                self.take(b'!');
                self.declaration()
            }
            _ => Ok(Kind::Invalid(Problem::BadMarkup)),
        }
    }

    /// After `<!`: a comment, a CDATA section or a declaration such as `DOCTYPE`.
    fn declaration(&mut self) -> Result<Kind, Failure> {
        match self.bytes.peek()? {
            Some(b'-') => {
                self.take(b'-');
                if self.bytes.peek()? != Some(b'-') {
                    return Ok(Kind::Invalid(Problem::BadMarkup));
                }
                self.take(b'-');
                self.mode = Mode::Comment;
                self.comment()
            }
            Some(b'[') => {
                self.take(b'[');
                for expected in *b"CDATA[" {
                    if self.bytes.peek()? != Some(expected) {
                        return Ok(Kind::Invalid(Problem::BadMarkup));
                    }
                    self.take(expected);
                }
                self.mode = Mode::CData;
                Ok(Kind::CDataOpen)
            }
            Some(byte) if byte.is_ascii_alphabetic() => {
                self.mode = Mode::Doctype { depth: 0, quote: 0 };
                self.doctype()
            }
            _ => Ok(Kind::Invalid(Problem::BadMarkup)),
        }
    }

    fn take_name(&mut self) -> Result<(), Failure> {
        while let Some(byte) = self.bytes.peek()? {
            if !is_name_char(byte) {
                break;
            }
            self.take(byte);
        }
        Ok(())
    }

    /// The name after `<` or `</`.
    pub(super) fn name(&mut self, end: bool, first: u8) -> Result<Kind, Failure> {
        self.mode = if end { Mode::EndTag } else { Mode::Tag };
        if !is_name_start(first) {
            return self.invalid_run(first);
        }
        self.take_name()?;
        Ok(Kind::Name)
    }

    pub(super) fn in_tag(&mut self, first: u8) -> Result<Kind, Failure> {
        match first {
            b'>' => {
                self.take(first);
                self.mode = Mode::Content;
                Ok(Kind::Close)
            }
            b'/' => {
                self.take(first);
                if self.bytes.peek()? != Some(b'>') {
                    return Ok(Kind::Invalid(Problem::Unexpected));
                }
                self.take(b'>');
                self.mode = Mode::Content;
                Ok(Kind::EmptyClose)
            }
            b'=' => {
                self.take(first);
                Ok(Kind::Eq)
            }
            b'"' | b'\'' => {
                self.take(first);
                self.mode = Mode::Value(first);
                self.value(first)
            }
            _ if is_name_start(first) => {
                self.take_name()?;
                Ok(Kind::AttrName)
            }
            _ => self.invalid_run(first),
        }
    }

    pub(super) fn in_end_tag(&mut self, first: u8) -> Result<Kind, Failure> {
        if first != b'>' {
            return self.invalid_run(first);
        }
        self.take(first);
        self.mode = Mode::Content;
        Ok(Kind::EndClose)
    }

    /// A run of bytes a tag has no place for, up to the next blank or markup byte; at least one.
    fn invalid_run(&mut self, first: u8) -> Result<Kind, Failure> {
        self.take(first);
        while let Some(byte) = self.bytes.peek()? {
            if is_blank(byte) || matches!(byte, b'<' | b'>' | b'/' | b'=' | b'"' | b'\'') {
                break;
            }
            self.take(byte);
        }
        Ok(Kind::Invalid(Problem::Unexpected))
    }

    /// The rest of a value piece: to its closing quote, or to an entity, a `<` or the line's end.
    pub(super) fn value(&mut self, quote: u8) -> Result<Kind, Failure> {
        while let Some(byte) = self.bytes.peek()? {
            if matches!(byte, b'&' | b'<' | b'\r' | b'\n') {
                break;
            }
            self.take(byte);
            if byte == quote {
                self.mode = Mode::Tag;
                return Ok(Kind::Value { closed: true });
            }
        }
        Ok(Kind::Value { closed: false })
    }

    pub(super) fn entity(&mut self) -> Result<Kind, Failure> {
        self.take(b'&');
        let named = match self.bytes.peek()? {
            Some(b'#') => {
                self.take(b'#');
                let hex = self.bytes.peek()? == Some(b'x');
                if hex {
                    self.take(b'x');
                }
                let mut digits = false;
                while let Some(byte) = self.bytes.peek()? {
                    let digit = if hex {
                        byte.is_ascii_hexdigit()
                    } else {
                        byte.is_ascii_digit()
                    };
                    if !digit {
                        break;
                    }
                    self.take(byte);
                    digits = true;
                }
                digits
            }
            Some(byte) if is_name_start(byte) => {
                self.take_name()?;
                true
            }
            _ => false,
        };
        if named && self.bytes.peek()? == Some(b';') {
            self.take(b';');
            return Ok(Kind::Entity);
        }
        Ok(Kind::Invalid(Problem::BadEntity))
    }

    /// A comment piece, to `-->` or the line's end.
    pub(super) fn comment(&mut self) -> Result<Kind, Failure> {
        let mut dashes = 0u32;
        while let Some(byte) = self.bytes.peek()? {
            if matches!(byte, b'\r' | b'\n') {
                break;
            }
            self.take(byte);
            if byte == b'>' && dashes >= 2 {
                self.mode = Mode::Content;
                return Ok(Kind::Comment { closed: true });
            }
            dashes = if byte == b'-' {
                dashes.saturating_add(1)
            } else {
                0
            };
        }
        Ok(Kind::Comment { closed: false })
    }

    /// A processing instruction's piece, to `?>` or the line's end.
    pub(super) fn pi(&mut self) -> Result<Kind, Failure> {
        let mut question = false;
        while let Some(byte) = self.bytes.peek()? {
            if matches!(byte, b'\r' | b'\n') {
                break;
            }
            self.take(byte);
            if byte == b'>' && question {
                self.mode = Mode::Content;
                return Ok(Kind::Pi { closed: true });
            }
            question = byte == b'?';
        }
        Ok(Kind::Pi { closed: false })
    }

    /// A CDATA section's piece, to `]]>` or the line's end.
    pub(super) fn cdata(&mut self) -> Result<Kind, Failure> {
        let mut brackets = 0u32;
        while let Some(byte) = self.bytes.peek()? {
            if matches!(byte, b'\r' | b'\n') {
                break;
            }
            self.take(byte);
            if byte == b'>' && brackets >= 2 {
                self.mode = Mode::Content;
                return Ok(Kind::CData { closed: true });
            }
            brackets = if byte == b']' {
                brackets.saturating_add(1)
            } else {
                0
            };
        }
        Ok(Kind::CData { closed: false })
    }

    /// A declaration's piece, to the `>` outside its literals and internal subset, or the line's
    /// end.
    pub(super) fn doctype(&mut self) -> Result<Kind, Failure> {
        let (mut depth, mut quote) = match self.mode {
            Mode::Doctype { depth, quote } => (depth, quote),
            _ => (0, 0),
        };
        while let Some(byte) = self.bytes.peek()? {
            if matches!(byte, b'\r' | b'\n') {
                break;
            }
            self.take(byte);
            if quote != 0 {
                if byte == quote {
                    quote = 0;
                }
                continue;
            }
            match byte {
                b'"' | b'\'' => quote = byte,
                b'[' => depth = depth.saturating_add(1),
                b']' => depth = depth.saturating_sub(1),
                b'>' if depth == 0 => {
                    self.mode = Mode::Content;
                    return Ok(Kind::Doctype { closed: true });
                }
                _ => {}
            }
        }
        self.mode = Mode::Doctype { depth, quote };
        Ok(Kind::Doctype { closed: false })
    }
}
