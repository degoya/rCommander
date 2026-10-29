//! PHP, from `<?php` or `<?=` to `?>`, over whatever HTML, CSS or JavaScript it is written into:
//! the frames below wait until `?>`. `?>` ends PHP in code and in a `//` or `#` comment, not in
//! a string or a `/* */` comment, as PHP reads it.
//!
//! Classes: keywords (in any case) `keyword`; `$variables` `key`, in a double-quoted string or
//! a heredoc too; strings, heredocs and nowdocs `string`, their markers `keyword`; numbers
//! `number`; `true false null` `literal`; comments `comment`; classes, interfaces, namespaces
//! and the built-in types `type-name`; functions where called or defined `function`; operators
//! `operator`; brackets, `;`, `,`, `<?php` and `?>` `punctuation`.

use super::scan::{is_blank, is_ident, is_ident_start};
use super::state::{Marker, PhpSub, fnv};
use super::words::{PHP_BEFORE_TYPE, PHP_KEYWORDS, PHP_LITERALS, PHP_TYPES};
use super::{Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

/// PHP's operators of more than one character, the longest first.
const OPERATORS: &[&[u8]] = &[
    b"<=>", b"**=", b"...", b"??=", b"<<=", b">>=", b"===", b"!==", b"?->", b"->", b"=>", b"::",
    b"==", b"!=", b"<>", b"<=", b">=", b"&&", b"||", b"??", b"++", b"--", b"+=", b"-=", b"*=",
    b"/=", b".=", b"%=", b"&=", b"|=", b"^=", b"**", b"<<", b">>",
];

/// The bytes looked at for a heredoc's opener or closing marker.
const MARKER_LOOK: usize = 256;

/// What one line of PHP keeps for itself.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Scratch {
    /// After `->`, `?->` or `::`: a property, a method or a constant.
    after_arrow: bool,
    /// After `class`, `new`, `extends` and the like: a class's or a namespace's name.
    type_next: bool,
    /// After `function`: the function's name.
    fn_next: bool,
    /// The blanks before a heredoc's closing marker were taken: the marker comes.
    marker_next: bool,
}

/// The length of the heredoc or nowdoc opener `bytes` start with (`<<<EOT`, `<<<"EOT"`,
/// `<<<'EOT'`) and its marker.
fn heredoc_open(bytes: &[u8]) -> Option<(usize, Marker)> {
    let rest = bytes.strip_prefix(b"<<<")?;
    let blanks = rest
        .iter()
        .take_while(|byte| matches!(**byte, b' ' | b'\t'))
        .count();
    let rest = rest.get(blanks..)?;
    let quote = rest
        .first()
        .copied()
        .filter(|byte| matches!(byte, b'\'' | b'"'));
    let rest = rest.get(usize::from(quote.is_some())..)?;
    let len = rest.iter().take_while(|byte| is_ident(**byte)).count();
    if len == 0 || !rest.first().is_some_and(|byte| is_ident_start(*byte)) {
        return None;
    }
    if let Some(quote) = quote
        && rest.get(len) != Some(&quote)
    {
        return None;
    }
    let total = 3 + blanks + len + 2 * usize::from(quote.is_some());
    let marker = Marker {
        hash: fnv(rest.get(..len)?),
        len: u16::try_from(len).ok()?,
        nowdoc: quote == Some(b'\''),
    };
    Some((total, marker))
}

/// The blanks before the heredoc's closing marker when `bytes`, a line's start, close it.
fn closes(bytes: &[u8], marker: Marker) -> Option<usize> {
    let blanks = bytes
        .iter()
        .take_while(|byte| matches!(**byte, b' ' | b'\t'))
        .count();
    let rest = bytes.get(blanks..)?;
    let len = rest.iter().take_while(|byte| is_ident(**byte)).count();
    let named = usize::from(marker.len) == len && rest.get(..len).map(fnv) == Some(marker.hash);
    named.then_some(blanks)
}

impl<S: Source> Lexer<'_, S> {
    pub(super) fn php(&mut self, sub: PhpSub, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        match sub {
            PhpSub::Code => self.php_code(byte, start),
            PhpSub::Comment => {
                if self.until(start, b"*/")? {
                    self.state.php = Some(PhpSub::Code);
                }
                Ok(self.lexeme(Some(TokenClass::Comment), start))
            }
            PhpSub::Str(quote) => self.php_text(quote, quote != b'\'', start),
            PhpSub::Heredoc(marker) => self.heredoc(marker, start),
        }
    }

    fn php_code(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        let next = self.peek_at(1)?.unwrap_or(0);
        if byte == b'?' && next == b'>' {
            self.take_n(2)?;
            self.state.php = None;
            self.scratch.rest = None;
            self.scratch.php = Scratch::default();
            return Ok(self.lexeme(Some(TokenClass::Punctuation), start));
        }
        if self.scratch.rest.is_some()
            || (byte == b'/' && next == b'/')
            || (byte == b'#' && next != b'[')
        {
            return self.php_line_comment(start);
        }
        if is_blank(byte) {
            return self.blank(start);
        }
        let class = match byte {
            b'/' if next == b'*' => {
                self.take_n(2)?;
                if !self.until(start, b"*/")? {
                    self.state.php = Some(PhpSub::Comment);
                }
                TokenClass::Comment
            }
            b'\'' | b'"' | b'`' => {
                self.take()?;
                self.state.php = Some(PhpSub::Str(byte));
                return self.php_text(byte, byte != b'\'', start);
            }
            b'$' if is_ident_start(next) => {
                self.take_word(start, b"")?;
                TokenClass::Key
            }
            b'<' if self.reader.look(3)? == b"<<<" => {
                let opener = heredoc_open(self.reader.look(MARKER_LOOK)?);
                let Some((len, marker)) = opener else {
                    return self.php_operator(start);
                };
                self.take_bytes(len)?;
                self.state.php = Some(PhpSub::Heredoc(marker));
                TokenClass::Keyword
            }
            _ if byte.is_ascii_digit() || (byte == b'.' && next.is_ascii_digit()) => {
                return self.number(start, false);
            }
            _ if is_ident_start(byte) || (byte == b'\\' && is_ident_start(next)) => {
                return self.php_word(start);
            }
            b'#' => {
                self.take_n(2)?;
                TokenClass::Punctuation
            }
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b';' | b',' => {
                self.take()?;
                self.scratch.php.type_next = false;
                self.scratch.php.fn_next = false;
                TokenClass::Punctuation
            }
            _ if b"=!<>&|+-*/%^~?.:@".contains(&byte) => return self.php_operator(start),
            _ => {
                self.take()?;
                TokenClass::Invalid
            }
        };
        Ok(self.lexeme(Some(class), start))
    }

    /// A `//` or `#` comment's piece, up to the line's end or `?>`.
    fn php_line_comment(&mut self, start: u32) -> Result<Lexeme, Failure> {
        self.scratch.rest = Some(TokenClass::Comment);
        self.take()?;
        while self.more(start)?.is_some() && self.reader.look(2)? != b"?>" {
            self.take()?;
        }
        Ok(self.lexeme(Some(TokenClass::Comment), start))
    }

    fn php_operator(&mut self, start: u32) -> Result<Lexeme, Failure> {
        let bytes = self.reader.look(3)?;
        let len = OPERATORS
            .iter()
            .find(|operator| bytes.starts_with(operator))
            .map_or(1, |operator| operator.len());
        self.scratch.php.after_arrow = matches!(bytes.get(..len), Some(b"->" | b"?->" | b"::"));
        self.take_n(len)?;
        Ok(self.lexeme(Some(TokenClass::Operator), start))
    }

    /// A name: a keyword, a type, a function, a constant.
    fn php_word(&mut self, start: u32) -> Result<Lexeme, Failure> {
        let word = self.word_ahead(b"\\")?;
        self.take_word(start, b"\\")?;
        let after = self.after_blanks()?;
        let scratch = &mut self.scratch.php;
        let after_arrow = std::mem::take(&mut scratch.after_arrow);
        let type_next = std::mem::take(&mut scratch.type_next);
        let fn_next = std::mem::take(&mut scratch.fn_next);
        let called = after[0] == b'(';
        let class = if after_arrow {
            called.then_some(TokenClass::Function)
        } else if word.is_in_any_case(PHP_LITERALS) {
            Some(TokenClass::Literal)
        } else if word.is_in_any_case(PHP_KEYWORDS) {
            scratch.type_next = word.is_in_any_case(PHP_BEFORE_TYPE);
            scratch.fn_next = word.is_in_any_case(&["function"]);
            Some(TokenClass::Keyword)
        } else if word.is_in_any_case(PHP_TYPES) || type_next || after == *b"::" {
            Some(TokenClass::TypeName)
        } else if fn_next || called {
            Some(TokenClass::Function)
        } else if word.is_capitalized() {
            Some(TokenClass::TypeName)
        } else {
            None
        };
        Ok(self.lexeme(class, start))
    }

    /// A string's or a heredoc's text: a piece up to its closing `quote` (none for a heredoc),
    /// `$variables` in their own pieces where `variables` holds.
    fn php_text(&mut self, quote: u8, variables: bool, start: u32) -> Result<Lexeme, Failure> {
        while let Some(byte) = self.more(start)? {
            if variables && byte == b'$' && self.peek_at(1)?.is_some_and(is_ident_start) {
                if self.col > start {
                    break;
                }
                self.take_word(start, b"")?;
                return Ok(self.lexeme(Some(TokenClass::Key), start));
            }
            self.take()?;
            if byte == b'\\' && quote != 0 {
                if self.more(start)?.is_some() {
                    self.take()?;
                }
            } else if quote != 0 && byte == quote {
                self.state.php = Some(PhpSub::Code);
                break;
            }
        }
        Ok(self.lexeme(Some(TokenClass::String), start))
    }

    /// A heredoc's line: its closing marker, or its text.
    fn heredoc(&mut self, marker: Marker, start: u32) -> Result<Lexeme, Failure> {
        let marker_next = std::mem::take(&mut self.scratch.php.marker_next);
        if start == 0 || marker_next {
            let blanks = closes(self.reader.look(MARKER_LOOK)?, marker);
            if let Some(blanks) = blanks {
                if blanks > 0 {
                    self.take_bytes(blanks)?;
                    self.scratch.php.marker_next = true;
                    return Ok(self.lexeme(None, start));
                }
                self.take_bytes(usize::from(marker.len))?;
                self.state.php = Some(PhpSub::Code);
                return Ok(self.lexeme(Some(TokenClass::Keyword), start));
            }
        }
        self.php_text(0, !marker.nowdoc, start)
    }
}

#[cfg(test)]
#[path = "php_tests.rs"]
mod tests;
