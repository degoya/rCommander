//! The scanners every language shares: blanks, the rest of a line, a piece up to a closer, a
//! string's body, a word and what follows it, numbers and HTML entities.

use super::{Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

/// The bytes of a word kept for comparing it with a language's words; a longer word is none of
/// them.
pub(super) const WORD_BYTES: usize = 32;

/// Whether `byte` goes on a name: ASCII letters, digits, `_`, `$`, and any byte of a character
/// beyond ASCII.
pub(super) fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') || byte >= 0x80
}

/// Whether `byte` starts a name.
pub(super) fn is_ident_start(byte: u8) -> bool {
    is_ident(byte) && !byte.is_ascii_digit()
}

/// A blank within a line.
pub(super) fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\x0c' | b'\x0b' | b'\r')
}

/// The start of the word ahead, for comparing it with a language's words.
#[derive(Clone, Copy, Debug)]
pub(super) struct Word {
    bytes: [u8; WORD_BYTES],
    len: usize,
    /// The word is longer than [`WORD_BYTES`]: none of a language's words.
    long: bool,
}

impl Word {
    /// The word `bytes` start with, `extra` bytes counting as part of it besides a name's.
    pub(super) fn of(bytes: &[u8], extra: &[u8]) -> Self {
        let mut word = Self {
            bytes: [0; WORD_BYTES],
            len: 0,
            long: false,
        };
        for byte in bytes {
            if !is_ident(*byte) && !extra.contains(byte) {
                break;
            }
            let Some(slot) = word.bytes.get_mut(word.len) else {
                word.long = true;
                break;
            };
            *slot = *byte;
            word.len += 1;
        }
        word
    }

    /// The word's bytes; empty for a long word.
    pub(super) fn text(&self) -> &[u8] {
        if self.long {
            return &[];
        }
        self.bytes.get(..self.len).unwrap_or_default()
    }

    /// Whether the word is one of `words`.
    pub(super) fn is_in(&self, words: &[&str]) -> bool {
        let text = self.text();
        !text.is_empty() && words.iter().any(|word| word.as_bytes() == text)
    }

    /// Whether the word is one of `words`, in any case.
    pub(super) fn is_in_any_case(&self, words: &[&str]) -> bool {
        let text = self.text();
        !text.is_empty()
            && words
                .iter()
                .any(|word| word.as_bytes().eq_ignore_ascii_case(text))
    }

    /// Whether the word starts with a capital letter.
    pub(super) fn starts_upper(&self) -> bool {
        self.bytes.first().is_some_and(u8::is_ascii_uppercase)
    }

    /// Whether the word starts with a capital and has a small letter: a class's name, not a
    /// constant's.
    pub(super) fn is_capitalized(&self) -> bool {
        let text = self.bytes.get(..self.len).unwrap_or_default();
        text.first().is_some_and(u8::is_ascii_uppercase) && text.iter().any(u8::is_ascii_lowercase)
    }
}

/// How a string's piece ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StrEnd {
    /// At its closing quote, taken.
    Closed,
    /// At the line's end right after a `\`: the string goes on on the next line.
    Escaped,
    /// At the line's end: the string is not closed.
    LineEnd,
    /// The piece is full, or another language's boundary comes: the string goes on.
    Cut,
}

impl<S: Source> Lexer<'_, S> {
    /// Blanks, from the one at `start`.
    pub(super) fn blank(&mut self, start: u32) -> Result<Lexeme, Failure> {
        self.take()?;
        while let Some(byte) = self.more(start)?
            && is_blank(byte)
        {
            self.take()?;
        }
        Ok(self.lexeme(None, start))
    }

    /// The rest of the line as `class`, up to a boundary.
    pub(super) fn rest_of_line(
        &mut self,
        class: TokenClass,
        start: u32,
    ) -> Result<Lexeme, Failure> {
        self.take()?;
        while self.more(start)?.is_some() {
            self.take()?;
        }
        Ok(self.lexeme(Some(class), start))
    }

    /// Text that is not coloured, up to the line's end or a boundary.
    pub(super) fn plain(&mut self, start: u32) -> Result<Lexeme, Failure> {
        self.take()?;
        while self.more(start)?.is_some() {
            self.take()?;
        }
        Ok(self.lexeme(None, start))
    }

    /// Takes characters up to and with `closer`, or to the line's end, a boundary or a full
    /// piece; whether the closer was taken.
    pub(super) fn until(&mut self, start: u32, closer: &[u8]) -> Result<bool, Failure> {
        loop {
            if self.more(start)?.is_none() {
                return Ok(false);
            }
            if self.reader.look(closer.len())? == closer {
                self.take_n(closer.len())?;
                return Ok(true);
            }
            self.take()?;
        }
    }

    /// Takes a string's characters after its opening `quote` up to and with its closing one; a
    /// `\` escapes the character after it.
    pub(super) fn string_body(&mut self, start: u32, quote: u8) -> Result<StrEnd, Failure> {
        loop {
            let Some(byte) = self.more(start)? else {
                return self.str_stop();
            };
            self.take()?;
            if byte == quote {
                return Ok(StrEnd::Closed);
            }
            if byte == b'\\' {
                if self.more(start)?.is_none() {
                    return Ok(if self.at_line_end()? {
                        StrEnd::Escaped
                    } else {
                        StrEnd::Cut
                    });
                }
                self.take()?;
            }
        }
    }

    /// Why a string's piece stopped before its quote.
    pub(super) fn str_stop(&mut self) -> Result<StrEnd, Failure> {
        Ok(if self.at_line_end()? {
            StrEnd::LineEnd
        } else {
            StrEnd::Cut
        })
    }

    /// Takes the characters of the next `count` bytes of the source.
    pub(super) fn take_bytes(&mut self, count: usize) -> Result<(), Failure> {
        let end = self.reader.offset() + count as u64;
        while self.reader.offset() < end && self.take()?.is_some() {}
        Ok(())
    }

    /// The word ahead, `extra` bytes counting as part of it.
    pub(super) fn word_ahead(&mut self, extra: &[u8]) -> Result<Word, Failure> {
        let bytes = self.reader.look(WORD_BYTES + 1)?;
        Ok(Word::of(bytes, extra))
    }

    /// Takes a word: the character next and every one after it that goes on a name or is one of
    /// `extra`.
    pub(super) fn take_word(&mut self, start: u32, extra: &[u8]) -> Result<(), Failure> {
        self.take()?;
        while let Some(byte) = self.more(start)?
            && (is_ident(byte) || extra.contains(&byte))
        {
            self.take()?;
        }
        Ok(())
    }

    /// The two bytes after the blanks ahead on this line, 0 for none: what follows a word.
    pub(super) fn after_blanks(&mut self) -> Result<[u8; 2], Failure> {
        let bytes = self.reader.look(64)?;
        let mut rest = bytes
            .iter()
            .skip_while(|byte| matches!(**byte, b' ' | b'\t'));
        Ok([
            rest.next().copied().unwrap_or(0),
            rest.next().copied().unwrap_or(0),
        ])
    }

    /// A number from the digit or `.` at `start`: `0x`, `0b` and `0o` forms, `_` between
    /// digits, a fraction, an exponent and the letters written right after it (`n`, `px`), with
    /// `percent` a `%` too.
    pub(super) fn number(&mut self, start: u32, percent: bool) -> Result<Lexeme, Failure> {
        let mut fraction = false;
        let mut prev = self.take()?.unwrap_or(0);
        fraction |= prev == b'.';
        let radix =
            prev == b'0' && matches!(self.peek()?, Some(b'x' | b'X' | b'b' | b'B' | b'o' | b'O'));
        while let Some(byte) = self.more(start)? {
            let exponent = !radix
                && matches!(prev, b'e' | b'E')
                && matches!(byte, b'+' | b'-')
                && self.peek_at(1)?.is_some_and(|after| after.is_ascii_digit());
            let point = byte == b'.'
                && !fraction
                && !radix
                && self.peek_at(1)?.is_some_and(|after| after.is_ascii_digit());
            if !(byte.is_ascii_alphanumeric()
                || byte == b'_'
                || (percent && byte == b'%')
                || exponent
                || point)
            {
                break;
            }
            fraction |= point;
            self.take()?;
            prev = byte;
        }
        Ok(self.lexeme(Some(TokenClass::Number), start))
    }

    /// An HTML entity at the `&` next - `&amp;`, `&#38;`, `&#x26;` - as `literal`; `None`, and
    /// nothing taken, when the `&` starts none.
    pub(super) fn entity(&mut self, start: u32) -> Result<Option<Lexeme>, Failure> {
        let bytes = self.reader.look(34)?;
        let len = entity_len(bytes);
        if len == 0 {
            return Ok(None);
        }
        self.take_n(len)?;
        Ok(Some(self.lexeme(Some(TokenClass::Literal), start)))
    }
}

/// The length of the entity `bytes` start with, 0 for none.
fn entity_len(bytes: &[u8]) -> usize {
    let body = bytes.get(1..).unwrap_or_default();
    let (skip, digits): (usize, fn(&u8) -> bool) = match body {
        [b'#', b'x' | b'X', ..] => (2, u8::is_ascii_hexdigit),
        [b'#', ..] => (1, u8::is_ascii_digit),
        [first, ..] if first.is_ascii_alphabetic() => (0, u8::is_ascii_alphanumeric),
        _ => return 0,
    };
    let name = body.get(skip..).unwrap_or_default();
    let count = name.iter().take_while(|&byte| digits(byte)).count();
    if count == 0 || name.get(count) != Some(&b';') {
        return 0;
    }
    1 + skip + count + 1
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
