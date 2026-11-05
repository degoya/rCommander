//! The tables: a line per entry, its columns by position.
//!
//! - **crontab** - five time fields (`*/5`, `0-23`, `mon-fri`) as `number`, then the command; a
//!   `@reboot`, `@daily` ... instead of the fields as `keyword`; `NAME=value` sets a variable
//!   (`key`). A comment is a line starting with `#`.
//! - **hosts** - an address as `number`, then its names as `key`; `#` starts a comment anywhere.

use super::{Emit, Lexer, LineState, is_blank};
use crate::types::TokenClass;

/// A crontab's time fields.
const TIME_FIELDS: usize = 5;

impl<E: Emit> Lexer<'_, E> {
    /// A crontab line.
    pub(crate) fn crontab(&mut self) -> LineState {
        match self.skip_blanks() {
            None => return LineState::default(),
            Some(b'#') => {
                self.comment();
                return LineState::default();
            }
            Some(_) => {}
        }
        let at = self.pos;
        let name_end = self.run(at, |byte| !is_blank(byte) && byte != b'=');
        if self.byte(at) == Some(b'@') {
            self.push(at, name_end, TokenClass::Keyword);
            return LineState::default();
        }
        self.pos = name_end;
        if self.skip_blanks() == Some(b'=') {
            self.push(at, name_end, TokenClass::Key);
            self.push(self.pos, self.pos + 1, TokenClass::Punctuation);
            self.pos += 1;
            if let Some(quote @ (b'"' | b'\'')) = self.skip_blanks() {
                let start = self.pos;
                self.pos += 1;
                self.string(start, quote);
            }
            return LineState::default();
        }
        self.pos = at;
        for _ in 0..TIME_FIELDS {
            if self.skip_blanks().is_none() {
                break;
            }
            let start = self.pos;
            self.pos = self.run(start, |byte| !is_blank(byte));
            self.push(start, self.pos, TokenClass::Number);
        }
        LineState::default()
    }

    /// A `hosts` line.
    pub(crate) fn hosts(&mut self) -> LineState {
        let mut class = TokenClass::Number;
        while let Some(byte) = self.skip_blanks() {
            if byte == b'#' {
                self.comment();
                break;
            }
            let start = self.pos;
            self.pos = self.run(start, |byte| !is_blank(byte) && byte != b'#');
            self.push(start, self.pos, class);
            class = TokenClass::Key;
        }
        LineState::default()
    }
}
