//! Strings and comments, inside which the lexer reads text: `"..."` and `@"..."@` with the
//! variables and `$( )` in them, `'...'` and `@'...'@` as they are, and `<# ... #>`. Each may go
//! on over lines; a here-string's closer stands at the start of a line.

use super::frames::Frame;
use super::{Emit, Lexer, words};
use crate::types::TokenClass;

impl<E: Emit> Lexer<'_, E> {
    /// At the start of a line inside a here-string: its closer, `"@` or `'@`, ends it.
    pub(super) fn here_end(&mut self) {
        let closer: &[u8] = match self.frames.top() {
            Some(Frame::HereDQ) => b"\"@",
            Some(Frame::HereSQ) => b"'@",
            _ => return,
        };
        if self.line.starts_with(closer) {
            self.push(0, 2, TokenClass::String);
            self.close();
            self.pos = 2;
        }
    }

    /// Text of `"..."` or of a `@"` here-string up to its end, a variable or `$(`.
    pub(super) fn expandable(&mut self) {
        let here = self.frames.top() == Some(Frame::HereDQ);
        let start = self.run_from.take().unwrap_or(self.pos);
        let mut at = self.pos;
        while let Some(byte) = self.byte(at) {
            match byte {
                b'`' => {
                    let next = self.byte(at + 1).map_or(0, words::char_width);
                    at = (at + 1 + next).min(self.line.len());
                }
                b'"' if !here && self.byte(at + 1) == Some(b'"') => at += 2,
                b'"' if !here => {
                    self.push(start, at + 1, TokenClass::String);
                    self.pos = at + 1;
                    self.close();
                    return;
                }
                b'$' if self.byte(at + 1) == Some(b'(') => {
                    self.push(start, at, TokenClass::String);
                    self.pos = at;
                    self.open_paren(2);
                    return;
                }
                b'$' => match words::variable_end(self.line, at) {
                    Some(end) => {
                        self.push(start, at, TokenClass::String);
                        let class = words::variable_class(&self.line[at..end]);
                        self.push(at, end, class);
                        self.pos = end;
                        return;
                    }
                    None => at += 1,
                },
                _ => at += 1,
            }
        }
        self.push(start, self.line.len(), TokenClass::String);
        self.pos = self.line.len();
    }

    /// Text of `'...'` up to its end; `''` is a quote in it.
    pub(super) fn single(&mut self) {
        let start = self.run_from.take().unwrap_or(self.pos);
        let mut at = self.pos;
        while let Some(byte) = self.byte(at) {
            match byte {
                b'\'' if self.byte(at + 1) == Some(b'\'') => at += 2,
                b'\'' => {
                    self.push(start, at + 1, TokenClass::String);
                    self.pos = at + 1;
                    self.close();
                    return;
                }
                _ => at += 1,
            }
        }
        self.push(start, self.line.len(), TokenClass::String);
        self.pos = self.line.len();
    }

    /// A line of a `@'` here-string: all text.
    pub(super) fn literal_line(&mut self) {
        let start = self.run_from.take().unwrap_or(self.pos);
        self.push(start, self.line.len(), TokenClass::String);
        self.pos = self.line.len();
    }

    /// Text of `<# ... #>` up to its end.
    pub(super) fn block_comment(&mut self) {
        let start = self.run_from.take().unwrap_or(self.pos);
        let rest = self.line.get(self.pos..).unwrap_or_default();
        match rest.windows(2).position(|pair| pair == b"#>") {
            Some(at) => {
                let end = self.pos + at + 2;
                self.push(start, end, TokenClass::Comment);
                self.pos = end;
                self.close();
            }
            None => {
                self.push(start, self.line.len(), TokenClass::Comment);
                self.pos = self.line.len();
            }
        }
    }

    /// `@"` or `@'` at `pos` opens a here-string when nothing but blanks follows it.
    pub(super) fn here_start(&mut self) -> bool {
        let frame = match self.byte(self.pos + 1) {
            Some(b'"') => Frame::HereDQ,
            Some(b'\'') => Frame::HereSQ,
            _ => return false,
        };
        let rest = self.line.get(self.pos + 2..).unwrap_or_default();
        if !rest.iter().all(|byte| words::is_blank(*byte)) {
            return false;
        }
        self.after_value();
        self.push(self.pos, self.pos + 2, TokenClass::String);
        self.open(frame, 2);
        self.run_from = None;
        self.pos = self.line.len();
        true
    }
}
