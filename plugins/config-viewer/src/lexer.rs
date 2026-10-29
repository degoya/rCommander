//! The lexer: one line of text at a time, in the state the line before left, for the `tokens` of
//! the lines on screen and for the open pass, which keeps the state at its checkpoints.
//!
//! A line is the decoded text of one line of the file, without its line break, so every span
//! starts and ends on a character of the text written. Most of these formats are a line at a
//! time; the state is what goes on past a line's end: a `\` continuing a properties value or a
//! registry hex list, a quoted `.env` value, and in TOML the open arrays and inline tables and a
//! multi-line string.

mod env;
mod ini;
mod reg;
mod toml;
mod toml_values;

use crate::types::{Format, Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The levels of TOML arrays and inline tables whose kind is kept; deeper ones are counted only.
const KEPT_LEVELS: u8 = 32;

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LineState {
    /// Nothing: a line of its own.
    #[default]
    Clear,
    /// A value continued with `\` from the line before: properties, a registry hex list.
    Continued,
    /// A `.env` value in this quote, still open.
    Quoted(u8),
    /// TOML with an array, an inline table or a multi-line string still open.
    Toml(TomlState),
}

/// TOML's open brackets and strings at a line's end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TomlState {
    /// Open arrays and inline tables.
    pub(crate) depth: u8,
    /// Bit `n` set: level `n + 1` is an inline table, else an array.
    pub(crate) braces: u32,
    pub(crate) string: Multi,
}

/// A TOML multi-line string still open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Multi {
    #[default]
    None,
    /// `"""`, with escapes.
    Basic,
    /// `'''`.
    Literal,
}

impl TomlState {
    /// Whether the innermost open level is an inline table, where keys come.
    pub(crate) fn in_table(self) -> bool {
        self.depth > 0 && self.depth <= KEPT_LEVELS && self.braces & (1 << (self.depth - 1)) != 0
    }

    pub(crate) fn open(&mut self, table: bool) {
        if self.depth < KEPT_LEVELS {
            let bit = 1u32 << self.depth;
            self.braces = if table {
                self.braces | bit
            } else {
                self.braces & !bit
            };
        }
        self.depth = self.depth.saturating_add(1);
    }

    /// Closes the innermost level; false when there is none, or it is of the other kind.
    pub(crate) fn close(&mut self, table: bool) -> bool {
        if self.depth == 0 {
            return false;
        }
        let matches = self.depth > KEPT_LEVELS || self.in_table() == table;
        self.depth -= 1;
        matches
    }

    /// The state a line ends in: `Clear` when nothing is open.
    pub(crate) fn end(self) -> LineState {
        if self.depth == 0 && self.string == Multi::None {
            LineState::Clear
        } else {
            LineState::Toml(self)
        }
    }
}

/// Lexes `text`, one line without its break, from `state`; pushes its tokens to `out` when
/// given. The state the next line starts in.
pub(crate) fn lex_line(
    text: &[u8],
    state: LineState,
    format: Format,
    out: Option<&mut Vec<Token>>,
) -> LineState {
    let mut out = Out { tokens: out };
    match format {
        Format::Ini => ini::ini(text, &mut out),
        Format::Lines => ini::lines(text, &mut out),
        Format::Registry => reg::reg(text, state, &mut out),
        Format::Env => env::env(text, state, &mut out),
        Format::Properties => env::properties(text, state, &mut out),
        Format::Toml => toml::toml(text, state, &mut out),
    }
}

/// Where a line's tokens go, when anywhere.
pub(crate) struct Out<'o> {
    tokens: Option<&'o mut Vec<Token>>,
}

impl Out<'_> {
    /// A token over bytes `start..end`; an empty one, or one past the line's limit, is dropped.
    pub(crate) fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        let Some(tokens) = self.tokens.as_deref_mut() else {
            return;
        };
        if end <= start || tokens.len() >= MAX_TOKENS_PER_LINE {
            return;
        }
        if let (Ok(start), Ok(len)) = (u32::try_from(start), u32::try_from(end - start)) {
            tokens.push(Token { start, len, class });
        }
    }
}

pub(crate) fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\x0C')
}

/// The first byte at or after `at` that is not a space.
pub(crate) fn skip_space(text: &[u8], mut at: usize) -> usize {
    while text.get(at).copied().is_some_and(is_space) {
        at += 1;
    }
    at
}

/// `to`, moved back over the spaces before it, never before `from`.
pub(crate) fn trim_end(text: &[u8], from: usize, to: usize) -> usize {
    let mut end = to.min(text.len());
    while end > from && text.get(end - 1).copied().is_some_and(is_space) {
        end -= 1;
    }
    end
}

/// After the character at `at`: the next character boundary of the text.
pub(crate) fn char_end(text: &[u8], at: usize) -> usize {
    let width = match text.get(at) {
        Some(0xF0..) => 4,
        Some(0xE0..) => 3,
        Some(0xC0..) => 2,
        _ => 1,
    };
    (at + width).min(text.len())
}

/// After the quote that closes a quoted part, searched from `from` (after the opening one); a
/// backslash takes the byte after it when `escapes`. `None` when the line ends first.
pub(crate) fn closing(text: &[u8], from: usize, quote: u8, escapes: bool) -> Option<usize> {
    let mut at = from;
    while let Some(&byte) = text.get(at) {
        if escapes && byte == b'\\' {
            at += 2;
            continue;
        }
        if byte == quote {
            return Some(at + 1);
        }
        at += 1;
    }
    None
}

/// The first comment mark of `marks` from `from` that follows a space: where an inline comment
/// starts.
pub(crate) fn comment_start(text: &[u8], from: usize, marks: &[u8]) -> Option<usize> {
    (from..text.len()).find(|&at| {
        text.get(at).is_some_and(|byte| marks.contains(byte))
            && at
                .checked_sub(1)
                .and_then(|before| text.get(before))
                .copied()
                .is_some_and(is_space)
    })
}

/// What follows a value or a header from `at`: nothing, a comment starting with one of `marks`,
/// or something that does not belong, `invalid`.
pub(crate) fn rest(text: &[u8], at: usize, marks: &[u8], out: &mut Out<'_>) {
    let at = skip_space(text, at);
    match text.get(at) {
        None => {}
        Some(byte) if marks.contains(byte) => out.push(at, text.len(), TokenClass::Comment),
        Some(_) => out.push(at, trim_end(text, at, text.len()), TokenClass::Invalid),
    }
}

/// The class of a bare value in INI, `.env` and properties: a number, a literal word
/// (`true false on off yes no`, any case), else a string.
pub(crate) fn scalar_class(word: &[u8]) -> TokenClass {
    if is_number(word) {
        TokenClass::Number
    } else if ["true", "false", "on", "off", "yes", "no"]
        .iter()
        .any(|literal| word.eq_ignore_ascii_case(literal.as_bytes()))
    {
        TokenClass::Literal
    } else {
        TokenClass::String
    }
}

/// A decimal integer or fraction with an optional sign, or `0x` hexadecimal.
fn is_number(word: &[u8]) -> bool {
    let unsigned = word
        .strip_prefix(b"-")
        .or_else(|| word.strip_prefix(b"+"))
        .unwrap_or(word);
    if let Some(hex) = unsigned
        .strip_prefix(b"0x")
        .or_else(|| unsigned.strip_prefix(b"0X"))
    {
        return !hex.is_empty() && hex.iter().all(u8::is_ascii_hexdigit);
    }
    let (whole, fraction) = match unsigned.iter().position(|byte| *byte == b'.') {
        Some(dot) => (&unsigned[..dot], Some(&unsigned[dot + 1..])),
        None => (unsigned, None),
    };
    let digits = |part: &[u8]| !part.is_empty() && part.iter().all(u8::is_ascii_digit);
    digits(whole) && fraction.is_none_or(digits)
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
