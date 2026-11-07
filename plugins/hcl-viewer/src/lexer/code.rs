//! Code: everything outside comment and template text, also inside template sequences - names,
//! numbers, operators, brackets, and where comments, quoted templates and heredocs start.

use super::words::{self, classify, is_blank, is_digit, is_name, is_name_start};
use super::{Emit, Frame, Lexer, Marker};
use crate::types::TokenClass;

impl<E: Emit> Lexer<'_, E> {
    /// The end of the run from `at` of bytes `keep` takes.
    fn run(&self, at: usize, keep: fn(u8) -> bool) -> usize {
        at + self
            .line
            .get(at..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| keep(**byte))
            .count()
    }

    /// One step through code.
    pub(super) fn code(&mut self) {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            return;
        };
        if is_blank(byte) {
            self.pos += 1;
            return;
        }
        let first = std::mem::replace(&mut self.first, false);
        let next = self.byte(at + 1);
        // `<<EOT` opens a heredoc; anything else after `<<` is two operators.
        if byte == b'<' && next == Some(b'<') && self.heredoc(at) {
            return;
        }
        match byte {
            b'#' => self.rest_of_line(at, TokenClass::Comment),
            b'/' if next == Some(b'/') => self.rest_of_line(at, TokenClass::Comment),
            b'/' if next == Some(b'*') => self.open_comment(at),
            b'"' => self.open_quoted(at),
            _ if is_digit(byte) => self.number(at),
            _ if is_name_start(byte) => self.word(at, first),
            b'{' => self.open_brace(at),
            b'}' => self.close_brace(at, 1),
            b'~' if next == Some(b'}') => self.close_brace(at, 2),
            b'(' | b')' | b'[' | b']' | b',' => self.punctuation(at, 1),
            b'.' if next == Some(b'.') && self.byte(at + 2) == Some(b'.') => self.operator(at, 3),
            b'.' => self.punctuation(at, 1),
            _ => match words::operator_width(self.line.get(at..).unwrap_or_default()) {
                Some(width) => self.operator(at, width),
                None => self.pos += 1,
            },
        }
    }

    fn punctuation(&mut self, at: usize, width: usize) {
        self.pos = at + width;
        self.push(at, self.pos, TokenClass::Punctuation);
    }

    fn operator(&mut self, at: usize, width: usize) {
        self.pos = at + width;
        self.push(at, self.pos, TokenClass::Operator);
    }

    /// `/*` opens a block comment.
    fn open_comment(&mut self, at: usize) {
        if !self.frames.push(Frame::Comment) {
            self.rest_of_line(at, TokenClass::Comment);
            return;
        }
        self.pos = at + 2;
        self.block_comment(at);
    }

    /// `"` opens a quoted template.
    fn open_quoted(&mut self, at: usize) {
        if !self.frames.push(Frame::Quoted) {
            self.rest_of_line(at, TokenClass::String);
            return;
        }
        self.pos = at + 1;
        self.template(at, true, true);
    }

    /// `<<` at `at`: a heredoc when a marker follows (`<<EOT`, `<<-EOT`) and nothing after it on
    /// the line, no heredoc is open already and the marker fits. Its lines start on the next.
    fn heredoc(&mut self, at: usize) -> bool {
        let name = at + 2 + usize::from(self.byte(at + 2) == Some(b'-'));
        if !self.byte(name).is_some_and(is_name_start) || self.frames.has_heredoc() {
            return false;
        }
        let end = self.run(name, is_name);
        let rest = self.line.get(end..).unwrap_or_default();
        if !rest.iter().all(|byte| matches!(byte, b' ' | b'\t' | b'\r')) {
            return false;
        }
        let Some(marker) = Marker::new(self.line.get(name..end).unwrap_or_default()) else {
            return false;
        };
        if !self.frames.push(Frame::Heredoc) {
            return false;
        }
        self.marker = marker;
        self.push(at, name, TokenClass::Operator);
        self.push(name, end, TokenClass::Keyword);
        self.pos = self.line.len();
        true
    }

    /// `{`: a block, an object, or a brace inside a template sequence, counted there so its own
    /// `}` is found.
    fn open_brace(&mut self, at: usize) {
        self.punctuation(at, 1);
        if let Some(Frame::Sequence { depth }) = self.frames.top() {
            self.frames.set_top(Frame::Sequence {
                depth: depth.saturating_add(1),
            });
        }
    }

    /// `}` (`width` 1) or `~}` (2): closes the template sequence on top when no brace is open in
    /// it.
    fn close_brace(&mut self, at: usize, width: usize) {
        self.punctuation(at, width);
        match self.frames.top() {
            Some(Frame::Sequence { depth: 0 }) => self.frames.pop(),
            Some(Frame::Sequence { depth }) => self.frames.set_top(Frame::Sequence {
                depth: depth.saturating_sub(1),
            }),
            _ => {}
        }
    }

    /// The number at `at`: `42`, `1.5`, `6.02e23`, `1E-3`.
    fn number(&mut self, at: usize) {
        let mut end = self.run(at, is_digit);
        if self.byte(end) == Some(b'.') && self.byte(end + 1).is_some_and(is_digit) {
            end = self.run(end + 1, is_digit);
        }
        if matches!(self.byte(end), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.byte(end + 1), Some(b'+' | b'-')));
            if self.byte(end + 1 + sign).is_some_and(is_digit) {
                end = self.run(end + 1 + sign, is_digit);
            }
        }
        self.pos = end;
        self.push(at, end, TokenClass::Number);
    }

    /// The first byte after `at` that is no blank.
    fn skip_blanks(&self, at: usize) -> usize {
        self.run(at, is_blank)
    }

    /// The word at `at`: an attribute's name (`=` follows), a block's type (the line's first
    /// word, labels and `{` follow), a keyword, a constant, a type, a function called, or a name;
    /// a name after `.` is an attribute of a value and stays uncoloured.
    fn word(&mut self, at: usize, first: bool) {
        let end = self.run(at, is_name);
        self.pos = end;
        let member = at > 0 && self.byte(at - 1) == Some(b'.');
        let after = self.skip_blanks(end);
        let assigned =
            self.byte(after) == Some(b'=') && !matches!(self.byte(after + 1), Some(b'=' | b'>'));
        let word = self.line.get(at..end).unwrap_or_default();
        let class = if member {
            None
        } else if assigned {
            Some(TokenClass::Key)
        } else if first && self.frames.depth() == 0 && self.block_header(end) {
            Some(TokenClass::Keyword)
        } else if let Some(class) = classify(word) {
            Some(class)
        } else if self.byte(end) == Some(b'(') {
            Some(TokenClass::Function)
        } else {
            None
        };
        if let Some(class) = class {
            self.push(at, end, class);
        }
    }

    /// Whether what follows a line's first word, from `end`, is a block's labels and its `{`:
    /// quoted labels and names, then the brace.
    fn block_header(&self, end: usize) -> bool {
        let mut at = end;
        loop {
            at = self.skip_blanks(at);
            match self.byte(at) {
                Some(b'{') => return true,
                Some(b'"') => {
                    let rest = self.line.get(at + 1..).unwrap_or_default();
                    let Some(close) = rest.iter().position(|byte| *byte == b'"') else {
                        return false;
                    };
                    at += close + 2;
                }
                Some(byte) if is_name_start(byte) => at = self.run(at, is_name),
                _ => return false,
            }
        }
    }
}
