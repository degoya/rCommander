//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The reader starts at the nearest checkpoint, and the lexer in the state the checkpoint kept,
//! so an error whose stack trace goes on into the lines asked for is still known. The lines before
//! the first asked for are lexed only for their state, and only when `level_colour` is `line` -
//! with `word` a line's tokens depend on nothing before it. The lines asked for are decoded into
//! the text `text` writes and lexed on it, so every offset is a UTF-8 byte of that text's line.

use rc_plugin_text::Encoding;

use crate::decode::{decode_line, line_text};
use crate::lexer::lex_line;
use crate::lines::LineIndex;
use crate::reader::{Reader, Unit};
use crate::types::{Failure, LevelColour, Source, Token};

pub use crate::lexer::MAX_TOKENS_PER_LINE;

/// The bytes of a line that are lexed; the rest of a longer line stays uncoloured. Even, so a
/// UTF-16 unit is never cut.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// The tokens of lines `first..first + wanted`, one list per line.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    encoding: Encoding,
    colour: LevelColour,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let point = lines.at_or_before(first);
    let mut reader = Reader::new(src, point.offset, LINE_CHUNK, Unit::of(encoding));
    let mut state = point.state;
    let mut raw = Vec::new();
    let mut text = String::new();
    let mut out = Vec::new();
    let mut line = point.line;
    while line < first + wanted {
        let Some(capped) = reader.read_line(MAX_LINE_BYTES, &mut raw)? else {
            break;
        };
        if line >= first {
            text.clear();
            decode_line(&raw, capped, encoding, &mut text);
            let mut tokens = Vec::new();
            state = lex_line(text.as_bytes(), state, colour, Some(&mut tokens));
            out.push(tokens);
        } else if colour == LevelColour::Line {
            state = lex_line(
                line_text(&raw, capped, encoding, &mut text),
                state,
                colour,
                None,
            );
        }
        line += 1;
    }
    Ok(out)
}
