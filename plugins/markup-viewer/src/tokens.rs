//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The reader starts at the nearest checkpoint, and the lexer in the state the checkpoint kept,
//! so an indented block, math or an entry begun lines before is still known. Every line is
//! decoded into the text `text` writes and lexed on it, with the next line read ahead beside it,
//! so every offset is a UTF-8 byte of that text's line. Math, a verbatim environment or a value
//! the file leaves open, or a blank line ends, has its opening marked `invalid`.

use rc_plugin_text::{Decoder, Encoding};

use crate::lang::Lang;
use crate::lexer::{LineState, QuoteOpen, lex_line};
use crate::lines::LineIndex;
use crate::reader::Reader;
use crate::types::{Failure, Source, Token, TokenClass};

pub use crate::lexer::MAX_TOKENS_PER_LINE;

/// The bytes of a line that are lexed; the rest of a longer line stays uncoloured, and the next
/// line starts in the state the lexed part left. Even, so a UTF-16 unit is never cut.
pub const MAX_LINE_BYTES: usize = 1 << 20;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// The text of one line, `bytes` without its line break. `capped`: the line goes on past
/// `bytes`, so a character cut at the end is left out rather than replaced, and the text is the
/// start of the line's whole text. A line break is a whole unit in every encoding, so the text
/// is the same as that line of the whole file's.
pub(crate) fn decode_line(bytes: &[u8], capped: bool, encoding: Encoding, out: &mut String) {
    let mut decoder = Decoder::new(encoding);
    decoder.push(bytes, out);
    if !capped {
        decoder.finish(out);
    }
}

/// The tokens of lines `first..first + wanted`, one list per line.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    lang: Lang,
    encoding: Encoding,
    unclosed: &[QuoteOpen],
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let point = lines.at_or_before(first);
    let mut reader = Reader::new(src, point.offset, LINE_CHUNK, encoding);
    let mut state: LineState = point.state;
    let mut bytes = Vec::new();
    let mut text = String::new();
    let mut next_text = String::new();
    let mut out = Vec::new();
    let mut line = point.line;
    let mut current = reader.read_line(MAX_LINE_BYTES, &mut bytes)?;
    if let Some(capped) = current {
        decode_line(&bytes, capped, encoding, &mut text);
    }
    while current.is_some() && line < first + wanted {
        let next = reader.read_line(MAX_LINE_BYTES, &mut bytes)?;
        next_text.clear();
        if let Some(capped) = next {
            decode_line(&bytes, capped, encoding, &mut next_text);
        }
        let ahead = next.map(|_| next_text.as_bytes());
        if line >= first {
            let mut tokens: Vec<Token> = Vec::new();
            state = lex_line(lang, text.as_bytes(), ahead, state, &mut tokens);
            mark_unclosed(&mut tokens, line, unclosed);
            out.push(tokens);
        } else {
            state = lex_line(lang, text.as_bytes(), ahead, state, &mut ());
        }
        std::mem::swap(&mut text, &mut next_text);
        current = next;
        line += 1;
    }
    Ok(out)
}

/// The opening of a string `unclosed` (in the order of the file) names on `line` splits off its
/// token as `invalid`.
fn mark_unclosed(tokens: &mut Vec<Token>, line: u64, unclosed: &[QuoteOpen]) {
    let from = unclosed.partition_point(|open| open.line < line);
    for open in unclosed[from..].iter().take_while(|open| open.line == line) {
        let Some(index) = tokens.iter().position(|token| token.start == open.col) else {
            continue;
        };
        let token = tokens[index];
        let width = u32::from(open.width).min(token.len);
        tokens[index] = Token {
            start: token.start,
            len: width,
            class: TokenClass::Invalid,
        };
        if token.len > width {
            tokens.insert(
                index + 1,
                Token {
                    start: token.start + width,
                    len: token.len - width,
                    class: token.class,
                },
            );
        }
    }
    tokens.truncate(MAX_TOKENS_PER_LINE);
}
