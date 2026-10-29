//! The tokens of one line for the `highlighted` output.
//!
//! A string is a `key` when a colon follows it - on its line, or as the first byte of the next
//! line that is not blank, which the caller looks up. Every span starts and ends at an ASCII byte
//! or the line's end, so it sits on character boundaries whatever the line holds.

use crate::lexer::{Kind, Lexer};
use crate::reader::SliceBytes;
use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// The tokens of `line` (without its line break). `capped`: the line goes on after these bytes,
/// so a token touching the end may be cut and is left out. `colon_follows`: the next line that is
/// not blank starts with `:`.
#[must_use]
pub fn tokenize_line(line: &[u8], capped: bool, colon_follows: bool) -> Vec<Token> {
    let mut lexer = Lexer::new(SliceBytes::new(line));
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
    let mut tokens = Vec::with_capacity(lexemes.len().min(MAX_TOKENS_PER_LINE));
    for (index, lexeme) in lexemes.iter().enumerate().take(MAX_TOKENS_PER_LINE) {
        let class = match lexeme.kind {
            Kind::String => {
                let colon = match lexemes.get(index + 1) {
                    Some(next) => next.kind == Kind::Colon,
                    None => complete && colon_follows,
                };
                if colon {
                    TokenClass::Key
                } else {
                    TokenClass::String
                }
            }
            Kind::Number => TokenClass::Number,
            Kind::Literal => TokenClass::Literal,
            Kind::Invalid(_) => TokenClass::Invalid,
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
