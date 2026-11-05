//! Comments of JSONC and JSON5 (RC-100-80): `//` to the end of the line, `/* */` over any
//! number of lines.

use super::{Kind, Lexeme, Lexer, OnLine, Problem};
use crate::reader::Bytes;
use crate::types::Failure;

impl<B: Bytes> Lexer<B> {
    /// Over the whole stream, at a `/`: skips the comment it starts; false when it starts none.
    /// A `/* */` that runs to the end is kept as `broken_comment`, for the next lexeme to name.
    pub(super) fn skip_comment(&mut self, on_line: &mut OnLine<'_>) -> Result<bool, Failure> {
        match self.bytes.peek_second()? {
            Some(b'/') => self.line_comment()?,
            Some(b'*') => {
                let at = (self.bytes.offset(), self.line, self.column);
                if !self.block_comment(on_line)? {
                    self.broken_comment = Some(at);
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// At a `/` that is a lexeme: a comment per line, else a lone `/`.
    pub(super) fn comment(&mut self, on_line: &mut OnLine<'_>) -> Result<Kind, Failure> {
        match self.bytes.peek_second()? {
            Some(b'/') => self.line_comment()?,
            Some(b'*') => {
                if !self.block_comment(on_line)? {
                    if !self.per_line {
                        return Ok(Kind::Invalid(Problem::UnterminatedComment));
                    }
                    self.in_block = true;
                }
            }
            _ => return self.unexpected(),
        }
        Ok(Kind::Comment)
    }

    /// Per line, at the start of a line inside a `/* */`: the comment up to its end or the line's.
    pub(super) fn continued_comment(&mut self) -> Result<Option<Lexeme>, Failure> {
        if self.bytes.peek()?.is_none() {
            return Ok(None);
        }
        self.captured.clear();
        let (start, line, column) = (self.bytes.offset(), self.line, self.column);
        self.in_block = !self.block_body(&mut |_, _, _| {})?;
        Ok(Some(Lexeme {
            kind: Kind::Comment,
            start,
            end: self.bytes.offset(),
            line,
            column,
        }))
    }

    /// `//` up to the line break, which stays.
    fn line_comment(&mut self) -> Result<(), Failure> {
        while let Some(byte) = self.bytes.peek()? {
            if byte == b'\n' {
                break;
            }
            self.take(byte);
        }
        Ok(())
    }

    /// `/*` and the rest; false when the stream ends first.
    fn block_comment(&mut self, on_line: &mut OnLine<'_>) -> Result<bool, Failure> {
        self.take(b'/');
        // `peek_second` saw the `*`; peeking again brings it into the window.
        self.bytes.peek()?;
        self.take(b'*');
        self.block_body(on_line)
    }

    /// The inside of a `/* */` up to and with its `*/`; false when the stream ends first. Every
    /// line it starts starts inside the comment.
    fn block_body(&mut self, on_line: &mut OnLine<'_>) -> Result<bool, Failure> {
        let mut star = false;
        while let Some(byte) = self.bytes.peek()? {
            if byte == b'\n' {
                self.newline(on_line, true);
                star = false;
                continue;
            }
            self.take(byte);
            if star && byte == b'/' {
                return Ok(true);
            }
            star = byte == b'*';
        }
        Ok(false)
    }
}
