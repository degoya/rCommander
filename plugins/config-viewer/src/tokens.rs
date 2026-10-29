//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The reader starts at the nearest checkpoint, and the lexer in the state the checkpoint kept, so
//! a continued value, a quoted `.env` value or a TOML array or multi-line string begun lines
//! before is still known. Every line is decoded into the text `text` writes and lexed on it, so
//! every offset is a UTF-8 byte of that text's line.

use crate::lexer::lex_line;
use crate::reader::{Newline, Reader};
use crate::text::decode_line;
use crate::tree::Doc;
use crate::types::{Failure, Source, Token};

pub use crate::lexer::MAX_TOKENS_PER_LINE;

/// The bytes of a line that are lexed; the rest of a longer line stays uncoloured and leaves the
/// state as it is.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The window `tokens` and the tree read through.
pub(crate) const LINE_CHUNK: u32 = 64 * 1024;

/// The tokens of lines `first..first + wanted`, one list per line.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    doc: Doc<'_>,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let point = doc.lines.at_or_before(first);
    let mut reader = Reader::new(src, point.offset, Newline::of(doc.encoding), LINE_CHUNK);
    let mut state = point.state;
    let (mut raw, mut text) = (Vec::new(), String::new());
    let mut out = Vec::new();
    let mut line = point.line;
    while line < first + wanted {
        let Some(capped) = reader.read_line(MAX_LINE_BYTES, &mut raw)? else {
            break;
        };
        text.clear();
        decode_line(&raw, capped, doc.encoding, &mut text);
        if line >= first {
            let mut tokens = Vec::new();
            state = lex_line(text.as_bytes(), state, doc.format, Some(&mut tokens));
            out.push(tokens);
        } else {
            state = lex_line(text.as_bytes(), state, doc.format, None);
        }
        line += 1;
    }
    Ok(out)
}
