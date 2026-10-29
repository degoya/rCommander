//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The reader starts at the nearest checkpoint, and the lexer in the state the checkpoint kept,
//! so a `^` continuation or a `(` block begun lines before is still known. The lines before the
//! first asked for are lexed on their bytes, only for their state; the lines asked for are
//! decoded into the text `text` writes and lexed on it, so every offset is a UTF-8 byte of that
//! text's line.

use crate::encoding::decode_line;
use crate::lexer::lex_line;
use crate::lines::LineIndex;
use crate::reader::Reader;
use crate::types::{Encoding, Failure, Source, Token};

pub use crate::lexer::MAX_TOKENS_PER_LINE;

/// The bytes of a line that are lexed (cmd.exe itself reads 8191 characters); the rest of a
/// longer line stays uncoloured and leaves the state as it is.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// The tokens of lines `first..first + wanted`, one list per line.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    encoding: Encoding,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let point = lines.at_or_before(first);
    let mut reader = Reader::new(src, point.offset, LINE_CHUNK);
    let mut state = point.state;
    let mut bytes = Vec::new();
    let mut text = String::new();
    let mut out = Vec::new();
    let mut line = point.line;
    while line < first + wanted {
        let Some(capped) = reader.read_line(MAX_LINE_BYTES, &mut bytes)? else {
            break;
        };
        if line >= first {
            text.clear();
            decode_line(&bytes, capped, encoding, &mut text);
            let mut tokens = Vec::new();
            state = lex_line(text.as_bytes(), state, Some(&mut tokens));
            out.push(tokens);
        } else {
            state = lex_line(&bytes, state, None);
        }
        line += 1;
    }
    Ok(out)
}
