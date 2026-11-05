//! A cursor over one line's text and the tokens it emits: the pieces every part of a Dockerfile
//! shares - blanks, variables, quoted strings, flags, heredoc markers.
//!
//! Every position the cursor stops at is an ASCII byte or the end of the text, and an escaped
//! character is skipped whole, so a token always starts and ends on a character boundary.

use super::{MAX_MARKER, Marker};
use crate::tokens::MAX_TOKENS_PER_LINE;
use crate::types::{Token, TokenClass};

pub(super) struct Cursor<'a> {
    text: &'a [u8],
    pub(super) at: usize,
    /// Where the lexed part ends: before an escape character continuing the line.
    end: usize,
    /// The escape character: `\`, or `` ` `` after `# escape=``.
    pub(super) escape: u8,
    tokens: &'a mut Vec<Token>,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(text: &'a [u8], end: usize, escape: u8, tokens: &'a mut Vec<Token>) -> Self {
        Self {
            text,
            at: 0,
            end: end.min(text.len()),
            escape,
            tokens,
        }
    }

    pub(super) fn peek(&self) -> Option<u8> {
        self.peek_at(0)
    }

    pub(super) fn peek_at(&self, ahead: usize) -> Option<u8> {
        let at = self.at + ahead;
        (at < self.end)
            .then(|| self.text.get(at).copied())
            .flatten()
    }

    pub(super) fn done(&self) -> bool {
        self.at >= self.end
    }

    /// The text from `start` to the cursor.
    pub(super) fn since(&self, start: usize) -> &'a [u8] {
        self.text.get(start..self.at).unwrap_or_default()
    }

    /// Whether the text ahead starts with `prefix`.
    pub(super) fn starts_with(&self, prefix: &[u8]) -> bool {
        self.text
            .get(self.at..self.end)
            .is_some_and(|rest| rest.starts_with(prefix))
    }

    pub(super) fn bump(&mut self, n: usize) {
        self.at = (self.at + n).min(self.end);
    }

    /// Skips spaces, tabs and the carriage return of a CRLF line.
    pub(super) fn blanks(&mut self) {
        while self.peek().is_some_and(is_blank) {
            self.at += 1;
        }
    }

    /// Takes bytes while `keep` holds for them (ASCII sets only).
    pub(super) fn take_while(&mut self, keep: impl Fn(u8) -> bool) {
        while self.peek().is_some_and(&keep) {
            self.at += 1;
        }
    }

    /// Takes everything to the end of the lexed part.
    pub(super) fn rest(&mut self) {
        self.at = self.end;
    }

    /// The token from `start` to the cursor, when it is not empty and the line has room.
    pub(super) fn emit(&mut self, start: usize, class: TokenClass) {
        self.span(start, self.at, class);
    }

    /// The token from `start` to `end`, wherever the cursor stands.
    pub(super) fn span(&mut self, start: usize, end: usize, class: TokenClass) {
        let (Ok(from), Ok(to)) = (u32::try_from(start), u32::try_from(end)) else {
            return;
        };
        if to > from && self.tokens.len() < MAX_TOKENS_PER_LINE {
            self.tokens.push(Token {
                start: from,
                len: to - from,
                class,
            });
        }
    }

    /// Takes `n` bytes as one token of `class`.
    pub(super) fn token(&mut self, n: usize, class: TokenClass) {
        let start = self.at;
        self.bump(n);
        self.emit(start, class);
    }

    /// At the escape character: takes it and the whole character it escapes.
    pub(super) fn escaped(&mut self) {
        self.bump(1);
        if let Some(next) = self.peek() {
            self.bump(char_len(next));
        }
    }

    /// At `$`: a variable as a key - `${...}`, `$NAME`, `$1` or a special parameter; false when
    /// the `$` begins none, and nothing is taken.
    pub(super) fn variable(&mut self) -> bool {
        let start = self.at;
        match self.peek_at(1) {
            Some(b'{') => {
                self.bump(2);
                self.take_while(|byte| byte != b'}');
                self.bump(usize::from(self.peek() == Some(b'}')));
            }
            Some(byte) if byte.is_ascii_alphabetic() || byte == b'_' => {
                self.bump(1);
                self.take_while(is_name_byte);
            }
            Some(byte) if byte.is_ascii_digit() || b"@*#?$!-".contains(&byte) => self.bump(2),
            _ => return false,
        }
        self.emit(start, TokenClass::Key);
        true
    }

    /// A quoted string from the cursor: at its opening `quote`, or inside one a line before left
    /// open (`opened` false). Variables in it are keys when `variables`. True when it closes on
    /// this line.
    pub(super) fn quoted(&mut self, quote: u8, opened: bool, variables: bool) -> bool {
        let mut start = self.at;
        if opened {
            self.bump(1);
        }
        while let Some(byte) = self.peek() {
            if byte == quote {
                self.bump(1);
                self.emit(start, TokenClass::String);
                return true;
            }
            if byte == self.escape && quote != b'\'' {
                self.escaped();
            } else if byte == b'$' && variables {
                let before = self.at;
                self.emit(start, TokenClass::String);
                if !self.variable() {
                    self.at = before;
                    self.bump(1);
                    start = before;
                    continue;
                }
                start = self.at;
            } else {
                self.bump(1);
            }
        }
        self.emit(start, TokenClass::String);
        false
    }

    /// A word up to the next blank: its plain parts of `class` (none for `None`), quoted parts
    /// strings, variables keys. The quote it leaves open at the end of the line, if any.
    pub(super) fn word(&mut self, class: Option<TokenClass>) -> Option<u8> {
        let mut start = self.at;
        let plain = |cursor: &mut Self, start: usize| {
            if let Some(class) = class {
                cursor.emit(start, class);
            }
        };
        while let Some(byte) = self.peek() {
            if is_blank(byte) {
                break;
            }
            match byte {
                b'"' | b'\'' => {
                    plain(self, start);
                    if !self.quoted(byte, true, byte == b'"') {
                        return Some(byte);
                    }
                    start = self.at;
                }
                b'$' => {
                    let before = self.at;
                    plain(self, start);
                    if !self.variable() {
                        self.at = before;
                        self.bump(1);
                        start = before;
                        continue;
                    }
                    start = self.at;
                }
                _ if byte == self.escape => self.escaped(),
                _ => self.bump(1),
            }
        }
        plain(self, start);
        None
    }

    /// At `--`: a flag - its name a key, `=` an operator, its value a string.
    pub(super) fn flag(&mut self) {
        let start = self.at;
        self.bump(2);
        self.take_while(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        self.emit(start, TokenClass::Key);
        if self.peek() == Some(b'=') {
            self.token(1, TokenClass::Operator);
            self.word(Some(TokenClass::String));
        }
    }

    /// At `<<`: a heredoc's operator and marker (`<<EOF`, `<<-EOF`, `<<"EOF"`), the marker kept for
    /// the body; `None` and nothing taken when no marker follows or it is too long.
    pub(super) fn heredoc(&mut self, shell: bool) -> Option<Marker> {
        let strip_tabs = self.peek_at(2) == Some(b'-');
        let operator = 2 + usize::from(strip_tabs);
        let quote = self
            .peek_at(operator)
            .filter(|byte| matches!(*byte, b'"' | b'\''));
        let name_at = self.at + operator + usize::from(quote.is_some());
        let mut name_end = name_at;
        while self
            .text
            .get(name_end)
            .is_some_and(|byte| name_end < self.end && is_name_byte(*byte))
        {
            name_end += 1;
        }
        let name = self.text.get(name_at..name_end).unwrap_or_default();
        if name.is_empty() || name.len() > MAX_MARKER {
            return None;
        }
        let mut marker = Marker {
            strip_tabs,
            shell,
            len: u8::try_from(name.len()).ok()?,
            ..Marker::default()
        };
        marker.bytes.get_mut(..name.len())?.copy_from_slice(name);
        self.token(operator, TokenClass::Operator);
        let start = self.at;
        self.at = name_end;
        if let Some(quote) = quote
            && self.peek() == Some(quote)
        {
            self.bump(1);
        }
        self.emit(start, TokenClass::Keyword);
        Some(marker)
    }
}

pub(super) fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r')
}

pub(super) fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The bytes of the UTF-8 character starting with `byte` (the text is valid UTF-8).
fn char_len(byte: u8) -> usize {
    match byte {
        0xF0.. => 4,
        0xE0.. => 3,
        0xC0.. => 2,
        _ => 1,
    }
}
