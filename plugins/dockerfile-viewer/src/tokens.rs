//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The lexer starts at the nearest checkpoint in the state the checkpoint kept, so a continued
//! instruction, an open quote or a heredoc begun lines before is still known. A line is lexed on
//! its text as written (`String::from_utf8_lossy`), so every span sits on its character
//! boundaries whatever the line holds.

use crate::lexer::{LineState, lex_line};
use crate::lines::LineIndex;
use crate::reader::Reader;
use crate::scan::Scanner;
use crate::types::{Failure, Source, Token};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The bytes of a line that get tokens; the rest of a longer line stays uncoloured.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// Lexes the line in `raw` from `state` into `tokens` (cleared first); the next line's state.
pub(crate) fn lex_raw(
    raw: &[u8],
    tail: Option<u8>,
    state: LineState,
    tokens: &mut Vec<Token>,
) -> LineState {
    tokens.clear();
    let text = String::from_utf8_lossy(raw);
    lex_line(text.as_bytes(), tail, state, tokens)
}

/// The tokens of lines `first..first + wanted`, one list per line.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let mut out: Vec<Vec<Token>> = (0..wanted).map(|_| Vec::new()).collect();
    let point = lines.at_or_before(first);
    let mut scanner = Scanner::new(Reader::new(src, point.offset, LINE_CHUNK));
    let (mut state, mut line) = (point.state, point.line);
    let (mut raw, mut skipped) = (Vec::new(), Vec::new());
    while line < first + wanted {
        let Some(found) = scanner.next_line(MAX_LINE_BYTES, &mut raw)? else {
            break;
        };
        let slot = line
            .checked_sub(first)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| out.get_mut(index));
        state = match slot {
            Some(tokens) => lex_raw(&raw, found.tail, state, tokens),
            None => lex_raw(&raw, found.tail, state, &mut skipped),
        };
        line += 1;
    }
    Ok(out)
}
