//! Code: words, numbers, operators, parameters and the openings of comments, strings, quoted
//! names and dollar-quoted bodies.

use super::words::{Word, classify};
use super::{Dialect, Emit, Lexer, Open, Tag};
use crate::types::TokenClass;

/// The most bytes a `[...]` name spans: T-SQL's 128 characters, of up to four bytes each, with
/// room for doubled `]]`. Searching no further keeps a line of many `[` linear.
const MAX_BRACKET_NAME: usize = 1024;

/// A byte of a word: a letter, a digit, `_`, `$` (PostgreSQL and MySQL allow it after the
/// first), or part of a character beyond ASCII.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') || byte >= 0x80
}

/// A byte a word starts with.
fn is_word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// A byte of a run of operators.
fn is_operator(byte: u8) -> bool {
    b"+-*/%<>=!|&^~?:@#".contains(&byte)
}

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

    /// Code from `pos` until something opens that the line may not close, or the line ends.
    pub(crate) fn code(&mut self) -> Open {
        while let Some(byte) = self.byte(self.pos) {
            let at = self.pos;
            let next = self.byte(at + 1);
            match byte {
                b' ' | b'\t' | b'\r' | 0x0B | 0x0C => self.pos += 1,
                b'-' if next == Some(b'-') => self.comment_to_end(at),
                b'#' if self.dialect == Dialect::MySql => self.comment_to_end(at),
                b'/' if next == Some(b'*') => {
                    self.pos += 2;
                    let open = self.block_comment(at, 1);
                    if open != Open::Code {
                        return open;
                    }
                }
                b'\'' | b'"' | b'`' => {
                    let backslash = self.dialect == Dialect::MySql && byte != b'`';
                    if let Some(open) = self.open_quote(at, byte, backslash) {
                        return open;
                    }
                }
                b'[' if self.bracket_name(at) => {
                    self.pos += 1;
                    self.quoted(at, b']', false);
                }
                b'$' => {
                    if let Some(open) = self.dollar_sign(at) {
                        return open;
                    }
                }
                b'@' if next.is_some_and(|byte| is_word(byte) || byte == b'@') => {
                    let name = self.run(at + 1, |byte| byte == b'@');
                    self.pos = self.run(name, is_word);
                    self.push(at, self.pos, TokenClass::Key);
                }
                b':' if next.is_some_and(is_word_start)
                    && (at == 0 || self.byte(at - 1) != Some(b':')) =>
                {
                    self.pos = self.run(at + 1, is_word);
                    self.push(at, self.pos, TokenClass::Key);
                }
                b'0'..=b'9' => self.number(at),
                b'.' if next.is_some_and(|byte| byte.is_ascii_digit()) => self.number(at),
                b'(' | b')' | b',' | b';' | b'.' | b'[' | b']' | b'{' | b'}' => {
                    self.pos += 1;
                    self.push(at, self.pos, TokenClass::Punctuation);
                }
                _ if is_word_start(byte)
                    || (byte == b'#'
                        && next.is_some_and(|byte| is_word_start(byte) || byte == b'#')) =>
                {
                    if let Some(open) = self.word(at) {
                        return open;
                    }
                }
                _ if is_operator(byte) => self.operators(at),
                _ => self.pos += 1,
            }
        }
        Open::Code
    }

    fn comment_to_end(&mut self, at: usize) {
        self.pos = self.line.len();
        self.push(at, self.pos, TokenClass::Comment);
    }

    /// The quote at `at` (after its prefix from `start`): the state when the line ends inside it.
    fn open_quote(&mut self, start: usize, quote: u8, backslash: bool) -> Option<Open> {
        self.pos += 1;
        match self.quoted(start, quote, backslash) {
            Open::Code => None,
            open => Some(open),
        }
    }

    /// Whether the `[` at `at` opens a T-SQL name (`[dbo].[Order Details]`) rather than an
    /// array subscript: it does not follow a word or a bracket, and the line closes it within
    /// [`MAX_BRACKET_NAME`] bytes.
    fn bracket_name(&self, at: usize) -> bool {
        let after_value = at
            .checked_sub(1)
            .and_then(|before| self.byte(before))
            .is_some_and(|byte| is_word(byte) || matches!(byte, b')' | b']'));
        let window = (at + 1 + MAX_BRACKET_NAME).min(self.line.len());
        let closed = self
            .line
            .get(at + 1..window)
            .and_then(|rest| rest.iter().position(|byte| *byte == b']'))
            .is_some_and(|close| close > 0);
        !after_value && closed
    }

    /// `$` at `at`: a dollar-quoted body (`$$`, `$tag$`), a parameter (`$1`), or an operator.
    fn dollar_sign(&mut self, at: usize) -> Option<Open> {
        let tag_end = self.run(at + 1, |byte| byte.is_ascii_alphanumeric() || byte == b'_');
        let tag = self.line.get(at + 1..tag_end).unwrap_or_default();
        let digits = !tag.is_empty() && tag.iter().all(u8::is_ascii_digit);
        if digits {
            self.pos = tag_end;
            self.push(at, self.pos, TokenClass::Key);
            return None;
        }
        let quotes =
            self.byte(tag_end) == Some(b'$') && !tag.first().is_some_and(u8::is_ascii_digit);
        match Tag::new(tag).filter(|_| quotes) {
            Some(tag) => {
                self.pos = tag_end + 1;
                match self.dollar(at, tag) {
                    Open::Code => None,
                    open => Some(open),
                }
            }
            None => {
                self.pos += 1;
                self.push(at, self.pos, TokenClass::Operator);
                None
            }
        }
    }

    /// The number at `at`: `42`, `1.5`, `.5`, `6.02e23`, `1E-3`, `0x1F`.
    fn number(&mut self, at: usize) {
        let digits = |byte: u8| byte.is_ascii_digit();
        let mut end =
            if self.byte(at) == Some(b'0') && matches!(self.byte(at + 1), Some(b'x' | b'X')) {
                self.run(at + 2, |byte| byte.is_ascii_hexdigit())
            } else {
                let whole = self.run(at, digits);
                if self.byte(whole) == Some(b'.') {
                    self.run(whole + 1, digits)
                } else {
                    whole
                }
            };
        if matches!(self.byte(end), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.byte(end + 1), Some(b'+' | b'-')));
            if self
                .byte(end + 1 + sign)
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                end = self.run(end + 1 + sign, digits);
            }
        }
        self.pos = end;
        if self.byte(end).is_some_and(is_word_start) {
            // `1abc` is one name in MySQL, not a number and a word.
            self.pos = self.run(end, is_word);
        } else {
            self.push(at, end, TokenClass::Number);
        }
    }

    /// The word at `at`: a string prefix (`N'...'`, `E'...'`, `X'...'`, `B'...'`), a keyword, a
    /// type, a literal, a function called (`count(`, the bracket right after it), or a name; a name
    /// after `.` is never a keyword.
    fn word(&mut self, at: usize) -> Option<Open> {
        // A T-SQL temporary table, `#name` or `##name`, is a name.
        let end = self.run(self.run(at, |byte| byte == b'#').max(at + 1), is_word);
        let word = self.line.get(at..end).unwrap_or_default();
        if self.byte(end) == Some(b'\'') && word.len() == 1 {
            let prefix = word[0].to_ascii_uppercase();
            if matches!(prefix, b'N' | b'E' | b'X' | b'B') {
                self.pos = end;
                let backslash = prefix == b'E' || self.dialect == Dialect::MySql;
                return self.open_quote(at, b'\'', backslash);
            }
        }
        self.pos = end;
        let qualified = at > 0 && self.byte(at - 1) == Some(b'.');
        let called = self.byte(end) == Some(b'(');
        let class = match classify(word) {
            _ if word.first() == Some(&b'#') => None,
            Word::Keyword if !qualified => Some(TokenClass::Keyword),
            Word::Type if !qualified => Some(TokenClass::TypeName),
            Word::Literal if !qualified => Some(TokenClass::Literal),
            _ if called => Some(TokenClass::Function),
            _ => None,
        };
        if let Some(class) = class {
            self.push(at, end, class);
        }
        None
    }

    /// A run of operators from `at`, stopping before a comment's `--` or `/*`.
    fn operators(&mut self, at: usize) {
        let mut end = at;
        while let Some(byte) = self.byte(end) {
            let pair = (byte, self.byte(end + 1));
            let comment = matches!(pair, (b'-', Some(b'-')) | (b'/', Some(b'*')))
                || (byte == b'#' && self.dialect == Dialect::MySql);
            if !is_operator(byte) || (comment && end > at) {
                break;
            }
            end += 1;
        }
        self.pos = end.max(at + 1);
        self.push(at, self.pos, TokenClass::Operator);
    }
}
