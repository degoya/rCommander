//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The reader starts at the nearest checkpoint, and the lexer in the state the checkpoint kept,
//! so an nginx statement or string, or an Apache line continued, begun lines before is still known. Every
//! line is decoded into the text `text` writes and lexed on it, so every offset is a UTF-8 byte of
//! that text's line.

use rc_plugin_text::{Decoder, Encoding};

use crate::format::Format;
use crate::lexer::{LineState, lex_line};
use crate::lines::LineIndex;
use crate::reader::Reader;
use crate::types::{Failure, Source, Token};

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
    encoding: Encoding,
    format: Format,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let point = lines.at_or_before(first);
    let mut reader = Reader::new(src, point.offset, LINE_CHUNK, encoding);
    let mut state: LineState = point.state;
    let mut bytes = Vec::new();
    let mut text = String::new();
    let mut out = Vec::new();
    let mut line = point.line;
    while line < first + wanted {
        let Some(capped) = reader.read_line(MAX_LINE_BYTES, &mut bytes)? else {
            break;
        };
        text.clear();
        decode_line(&bytes, capped, encoding, &mut text);
        if line >= first {
            let mut tokens: Vec<Token> = Vec::new();
            state = lex_line(text.as_bytes(), state, format, &mut tokens);
            out.push(tokens);
        } else {
            state = lex_line(text.as_bytes(), state, format, &mut ());
        }
        line += 1;
    }
    Ok(out)
}
