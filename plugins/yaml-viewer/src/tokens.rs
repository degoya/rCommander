//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The lexer starts at the nearest checkpoint in the state the checkpoint kept, so a block
//! scalar, a quoted scalar or a flow collection begun lines before is still known. A scalar is a
//! `key` when a `:` follows it on its line - YAML allows no other kind of implicit key - a plain
//! one a `number` or `literal` when its text is one, else it stays uncoloured. Every span starts
//! and ends at an ASCII byte or the line's end of the file's bytes; a line that is not valid UTF-8
//! is read again up to its last token, and its spans moved onto the text written, where a byte
//! that is not UTF-8 is U+FFFD (`src/utf8.rs`).

use crate::lexer::{Kind, Lexeme, Lexer};
use crate::lines::LineIndex;
use crate::reader::Reader;
use crate::types::{Failure, Source, Token, TokenClass};
use crate::utf8;

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The bytes of a line that get tokens; the rest of a longer line stays uncoloured.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;
/// The bytes of a plain scalar looked at to tell a number or literal: none is longer.
const CLASSIFY_BYTES: usize = 64;

/// The tokens of lines `first..first + wanted`, one list per line.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let mut out: Vec<Vec<Token>> = (0..wanted).map(|_| Vec::new()).collect();
    // Where each line asked for starts, from a lexeme on it.
    let mut starts: Vec<Option<u64>> = vec![None; out.len()];
    let point = lines.at_or_before(first);
    let mut lexer = Lexer::resume(
        Reader::new(src, point.offset, LINE_CHUNK),
        point.line,
        point.state,
    )
    .with_capture(CLASSIFY_BYTES);
    let last = first + wanted;
    // The last scalar, waiting to see whether a `:` makes it a key.
    let mut held: Option<(Lexeme, Option<TokenClass>)> = None;
    while let Some(lexeme) = lexer.next()? {
        let line = lexeme.line - 1;
        if line >= last {
            break;
        }
        if let Some(start) = line
            .checked_sub(first)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| starts.get_mut(index))
        {
            start.get_or_insert(lexeme.start.saturating_sub(u64::from(lexeme.col)));
        }
        if let Some((before, class)) = held.take() {
            let key = lexeme.kind == Kind::Colon
                && lexeme.line == before.line
                && matches!(before.kind, Kind::Plain | Kind::Quoted);
            push(
                &mut out,
                first,
                &before,
                if key { Some(TokenClass::Key) } else { class },
            );
        }
        let class = classify(&lexeme, lexer.captured());
        if matches!(lexeme.kind, Kind::Plain | Kind::Quoted) {
            held = Some((lexeme, class));
        } else {
            push(&mut out, first, &lexeme, class);
        }
        if lexeme.col as usize >= MAX_LINE_BYTES
            && let Some(next) = lines.exact(line + 1)
        {
            // The rest of a long line: on at the checkpoint after it.
            if let Some((before, class)) = held.take() {
                push(&mut out, first, &before, class);
            }
            lexer.seek_line(next.offset, next.line, next.state);
        }
    }
    if let Some((before, class)) = held {
        push(&mut out, first, &before, class);
    }
    drop(lexer);
    for (tokens, start) in out.iter_mut().zip(&starts) {
        // The line's bytes up to its last token's end: they say where its characters are.
        let (Some(start), Some(end)) = (start, tokens.last().map(|token| token.start + token.len))
        else {
            continue;
        };
        let bytes = src.read_at(*start, end)?;
        utf8::to_text(&bytes, tokens);
    }
    Ok(out)
}

/// Adds the token of `lexeme` to its line's list when that line was asked for and has room.
fn push(out: &mut [Vec<Token>], first: u64, lexeme: &Lexeme, class: Option<TokenClass>) {
    let Some(class) = class else {
        return;
    };
    let Some(tokens) = (lexeme.line - 1)
        .checked_sub(first)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| out.get_mut(index))
    else {
        return;
    };
    let start = lexeme.col as usize;
    let Ok(len) = u32::try_from(lexeme.end - lexeme.start) else {
        return;
    };
    if tokens.len() >= MAX_TOKENS_PER_LINE || start + len as usize > MAX_LINE_BYTES {
        return;
    }
    tokens.push(Token {
        start: lexeme.col,
        len,
        class,
    });
}

/// The class of a lexeme, a scalar's before it may turn out a key; `None` for a plain scalar of
/// no kind. `captured`: the lexeme's first bytes.
fn classify(lexeme: &Lexeme, captured: &[u8]) -> Option<TokenClass> {
    Some(match lexeme.kind {
        Kind::Plain => {
            let length = usize::try_from(lexeme.end - lexeme.start).ok()?;
            return scalar_class(captured.get(..length)?);
        }
        Kind::Quoted | Kind::QuotedLine | Kind::BlockHeader | Kind::BlockText => TokenClass::String,
        Kind::Comment => TokenClass::Comment,
        Kind::DocStart | Kind::DocEnd | Kind::Directive => TokenClass::Keyword,
        Kind::Anchor | Kind::Alias | Kind::Tag => TokenClass::TypeName,
        Kind::Dash
        | Kind::Question
        | Kind::Colon
        | Kind::FlowOpen
        | Kind::FlowClose
        | Kind::Comma => TokenClass::Punctuation,
        Kind::Invalid(_) => TokenClass::Invalid,
    })
}

/// A plain scalar's class: a number or literal of the YAML 1.2 core schema (with the YAML 1.1
/// booleans `yes`, `no`, `on`, `off` editors colour too), else none.
pub(crate) fn scalar_class(text: &[u8]) -> Option<TokenClass> {
    if is_number(text) {
        return Some(TokenClass::Number);
    }
    let literal = matches!(
        text,
        b"~" | b"null"
            | b"Null"
            | b"NULL"
            | b"true"
            | b"True"
            | b"TRUE"
            | b"false"
            | b"False"
            | b"FALSE"
            | b"yes"
            | b"Yes"
            | b"YES"
            | b"no"
            | b"No"
            | b"NO"
            | b"on"
            | b"On"
            | b"ON"
            | b"off"
            | b"Off"
            | b"OFF"
    );
    literal.then_some(TokenClass::Literal)
}

/// An integer (decimal, `0o` octal, `0x` hexadecimal), a float, `.inf` or `.nan`.
fn is_number(text: &[u8]) -> bool {
    let digits = |part: &[u8]| part.iter().all(u8::is_ascii_digit);
    if let Some(octal) = text.strip_prefix(b"0o") {
        return !octal.is_empty() && octal.iter().all(|byte| (b'0'..=b'7').contains(byte));
    }
    if let Some(hex) = text.strip_prefix(b"0x") {
        return !hex.is_empty() && hex.iter().all(u8::is_ascii_hexdigit);
    }
    if matches!(text, b".nan" | b".NaN" | b".NAN") {
        return true;
    }
    let unsigned = unsign(text);
    if matches!(unsigned, b".inf" | b".Inf" | b".INF") {
        return true;
    }
    let (mantissa, exponent) = match unsigned.iter().position(|byte| matches!(byte, b'e' | b'E')) {
        Some(at) => {
            let (mantissa, exponent) = unsigned.split_at(at);
            (mantissa, exponent.get(1..))
        }
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.iter().position(|byte| *byte == b'.') {
        Some(at) => {
            let (whole, fraction) = mantissa.split_at(at);
            (whole, fraction.get(1..))
        }
        None => (mantissa, None),
    };
    let mantissa_ok = digits(whole)
        && fraction.is_none_or(digits)
        && (!whole.is_empty() || fraction.is_some_and(|fraction| !fraction.is_empty()));
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = unsign(exponent);
        !exponent.is_empty() && digits(exponent)
    });
    mantissa_ok && exponent_ok
}

/// `text` without a leading sign.
fn unsign(text: &[u8]) -> &[u8] {
    text.strip_prefix(b"-")
        .or_else(|| text.strip_prefix(b"+"))
        .unwrap_or(text)
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
