//! The tokens of one line for the `highlighted` output.
//!
//! A string is a `key` when a colon follows it - on its line, or as the first byte of the next
//! line that is not blank, which the caller looks up. Every span starts and ends at an ASCII byte
//! or the line's end, so it sits on character boundaries whatever the line holds.
//!
//! In JSONC and JSON5 a comment is a `comment` and a line may start inside one; a JSON5 name
//! before a colon is a `key`. In JSON Lines a line is checked as the value it is: from where it
//! stops being valid, its tokens are `invalid`, and no other line's are (RC-100-80).

use crate::dialect::Dialect;
use crate::lexer::{Kind, Lexeme, Lexer};
use crate::reader::SliceBytes;
use crate::structure::{Builder, is_key};
use crate::types::{Token, TokenClass};

/// How one line is read beside its bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineOptions {
    pub dialect: Dialect,
    /// The line starts inside a `/* */` comment.
    pub in_comment: bool,
    /// The line is a value of its own (JSON Lines, as-is): checked, and invalid from its error.
    pub check: bool,
}

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// The tokens of `line` (without its line break). `capped`: the line goes on after these bytes,
/// so a token touching the end may be cut and is left out. `colon_follows`: the next line that is
/// not blank starts with `:`.
#[must_use]
pub fn tokenize_line(line: &[u8], capped: bool, colon_follows: bool) -> Vec<Token> {
    tokenize_line_with(line, capped, colon_follows, LineOptions::default())
}

/// [`tokenize_line`] in `options`' dialect.
#[must_use]
pub fn tokenize_line_with(
    line: &[u8],
    capped: bool,
    colon_follows: bool,
    options: LineOptions,
) -> Vec<Token> {
    let dialect = options.dialect;
    let mut lexer = Lexer::new(SliceBytes::new(line))
        .dialect(dialect)
        .per_line(options.in_comment);
    let mut lexemes = Vec::new();
    // One more than kept: the colon that makes the last kept string a key.
    while lexemes.len() <= MAX_TOKENS_PER_LINE {
        match lexer.next() {
            Ok(Some(lexeme)) => lexemes.push(lexeme),
            Ok(None) | Err(_) => break,
        }
    }
    let complete = lexemes.len() <= MAX_TOKENS_PER_LINE && !capped;
    if capped
        && lexemes
            .last()
            .is_some_and(|last| last.end == line.len() as u64)
    {
        lexemes.pop();
    }
    let invalid_from = if options.check {
        first_error(&lexemes, complete)
    } else {
        usize::MAX
    };
    let mut tokens = Vec::with_capacity(lexemes.len().min(MAX_TOKENS_PER_LINE));
    for (index, lexeme) in lexemes.iter().enumerate().take(MAX_TOKENS_PER_LINE) {
        let colon = || {
            // The next lexeme but comments.
            match lexemes
                .iter()
                .skip(index + 1)
                .find(|next| next.kind != Kind::Comment)
            {
                Some(next) => next.kind == Kind::Colon,
                None => complete && colon_follows,
            }
        };
        let class = match lexeme.kind {
            _ if index >= invalid_from => TokenClass::Invalid,
            kind if is_key(kind, dialect) && colon() => TokenClass::Key,
            Kind::String => TokenClass::String,
            Kind::Number => TokenClass::Number,
            Kind::Literal => TokenClass::Literal,
            Kind::Comment => TokenClass::Comment,
            // A name is a key, never a value.
            Kind::Identifier | Kind::Invalid(_) => TokenClass::Invalid,
            Kind::BeginObject
            | Kind::EndObject
            | Kind::BeginArray
            | Kind::EndArray
            | Kind::Comma
            | Kind::Colon => TokenClass::Punctuation,
        };
        let (Ok(start), Ok(len)) = (
            u32::try_from(lexeme.start),
            u32::try_from(lexeme.end - lexeme.start),
        ) else {
            break;
        };
        tokens.push(Token { start, len, class });
    }
    tokens
}

/// The index of the first lexeme of `lexemes` that does not fit the grammar of one value; the
/// last when the line ends before its value does (`complete`: these are all of the line's).
fn first_error(lexemes: &[Lexeme], complete: bool) -> usize {
    let mut builder = Builder::new();
    for (index, lexeme) in lexemes.iter().enumerate() {
        builder.feed(lexeme);
        if builder.failed() {
            return index;
        }
    }
    if complete && !lexemes.is_empty() && !builder.complete() {
        return lexemes.len() - 1;
    }
    usize::MAX
}

/// Whether `line`, which starts inside a `/* */` comment when `in_comment`, ends inside one.
pub(crate) fn ends_in_comment(line: &[u8], dialect: Dialect, in_comment: bool) -> bool {
    let mut lexer = Lexer::new(SliceBytes::new(line))
        .dialect(dialect)
        .per_line(in_comment);
    while let Ok(Some(_)) = lexer.next() {}
    lexer.in_block_comment()
}

/// Whether `line` holds nothing but whitespace.
pub(crate) fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
}

/// Whether the first line of `following` that is not blank starts with `:`.
pub(crate) fn colon_follows<'a>(following: impl IntoIterator<Item = &'a [u8]>) -> bool {
    following
        .into_iter()
        .find(|line| !is_blank(line))
        .and_then(|line| {
            line.iter()
                .find(|byte| !matches!(byte, b' ' | b'\t' | b'\r'))
        })
        == Some(&b':')
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
