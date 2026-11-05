//! The grammars of directives and blocks: nginx, Apache and OpenSSH.
//!
//! - **nginx** - a statement is a directive and its values up to `;`, or up to `{` for a block
//!   (`server`, `location`, `upstream` ...), and may go on over lines; `}` closes a block. The
//!   directive is `key`, a block's `keyword`. `#` starts a comment where a word would start.
//! - **Apache** - a directive and its values per line, a backslash continuing it; a section is
//!   `<Tag args>` ... `</Tag>`, its tags `keyword`. A comment is a line starting with `#`.
//! - **OpenSSH** - `Keyword value` or `Keyword=value` per line; `Host` and `Match` start a block
//!   and are `keyword`, every other keyword `key`. A comment is a line starting with `#`.

use super::{Emit, Lexer, LineState, is_blank};
use crate::types::TokenClass;

/// Whether `line` ends in a backslash that is not itself escaped.
fn ends_continued(line: &[u8]) -> bool {
    line.iter().rev().take_while(|byte| **byte == b'\\').count() % 2 == 1
}

impl<E: Emit> Lexer<'_, E> {
    /// An nginx line.
    pub(crate) fn nginx(&mut self, state: LineState) -> LineState {
        let mut statement = state.statement;
        if state.quote != 0 && !self.string(0, state.quote) {
            return state;
        }
        while let Some(byte) = self.skip_blanks() {
            let at = self.pos;
            match byte {
                b'#' => self.comment(),
                b';' | b'{' | b'}' => {
                    self.pos += 1;
                    self.push(at, self.pos, TokenClass::Punctuation);
                    statement = false;
                }
                b'(' | b')' => {
                    self.pos += 1;
                    self.push(at, self.pos, TokenClass::Punctuation);
                }
                _ if !statement => {
                    let end = self.run(at, |byte| !self.breaks_word(byte)).max(at + 1);
                    let class = if self.opens_block(end) {
                        TokenClass::Keyword
                    } else {
                        TokenClass::Key
                    };
                    self.push(at, end, class);
                    self.pos = end;
                    statement = true;
                }
                _ => {
                    let quote = self.value();
                    if quote != 0 {
                        return LineState {
                            statement: true,
                            quote,
                        };
                    }
                }
            }
        }
        LineState {
            statement,
            quote: 0,
        }
    }

    /// Whether the statement from `from` opens a block: a `{` comes before a `;` on its line,
    /// outside quotes and comments.
    fn opens_block(&self, from: usize) -> bool {
        let mut quote = None;
        let mut at = from;
        while let Some(byte) = self.byte(at) {
            match (quote, byte) {
                (Some(_), b'\\') => at += 1,
                (Some(open), _) if byte == open => quote = None,
                (Some(_), _) => {}
                (None, b'"' | b'\'') => quote = Some(byte),
                (None, b'{') => return true,
                (None, b';' | b'}') => return false,
                (None, b'#') if self.byte(at - 1).is_some_and(is_blank) => return false,
                (None, _) => {}
            }
            at += 1;
        }
        false
    }

    /// An Apache line.
    pub(crate) fn apache(&mut self, state: LineState) -> LineState {
        let mut section = false;
        if !state.statement {
            match self.skip_blanks() {
                None => return LineState::default(),
                Some(b'#') => {
                    self.comment();
                    return LineState::default();
                }
                Some(b'<') => {
                    let at = self.pos;
                    let end = self.run(at + 1, |byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b':' | b'.')
                    });
                    self.push(at, end, TokenClass::Keyword);
                    self.pos = end;
                    section = true;
                }
                Some(_) => self.directive(),
            }
        }
        while let Some(byte) = self.skip_blanks() {
            if section && byte == b'>' {
                self.push(self.pos, self.pos + 1, TokenClass::Keyword);
                self.pos += 1;
            } else {
                self.value();
            }
        }
        LineState {
            statement: ends_continued(self.line),
            quote: 0,
        }
    }

    /// The directive at `pos`, as `key`.
    fn directive(&mut self) {
        let at = self.pos;
        let end = self.run(at, |byte| !is_blank(byte) && byte != b'=');
        self.push(at, end, TokenClass::Key);
        self.pos = end;
    }

    /// An OpenSSH line.
    pub(crate) fn ssh(&mut self) -> LineState {
        match self.skip_blanks() {
            None => return LineState::default(),
            Some(b'#') => {
                self.comment();
                return LineState::default();
            }
            Some(_) => {}
        }
        let at = self.pos;
        let end = self.run(at, |byte| !is_blank(byte) && byte != b'=');
        let keyword = &self.line[at..end];
        let class =
            if keyword.eq_ignore_ascii_case(b"Host") || keyword.eq_ignore_ascii_case(b"Match") {
                TokenClass::Keyword
            } else {
                TokenClass::Key
            };
        self.push(at, end, class);
        self.pos = end;
        if self.skip_blanks() == Some(b'=') {
            self.push(self.pos, self.pos + 1, TokenClass::Punctuation);
            self.pos += 1;
        }
        while self.skip_blanks().is_some() {
            self.value();
        }
        LineState::default()
    }
}
