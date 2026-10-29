//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The lexer starts at the nearest checkpoint in the state the checkpoint kept, so a comment, a
//! string, a `<script>` or a PHP block begun lines before is still known. Every span starts and
//! ends on a character of the text written, so it sits on character boundaries whatever the
//! line holds.

use crate::lexer::{Lexeme, Lexer};
use crate::lines::LineIndex;
use crate::reader::Reader;
use crate::types::{Failure, Source, Token};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The bytes of a line that get tokens; the rest of a longer line stays uncoloured.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// The tokens of lines `first..first + wanted`, one list per line; `php_file` as the document's.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    php_file: bool,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let mut out: Vec<Vec<Token>> = (0..wanted).map(|_| Vec::new()).collect();
    let point = lines.at_or_before(first);
    let mut lexer = Lexer::resume(
        Reader::new(src, point.offset, LINE_CHUNK),
        point.line,
        lines.state(&point),
        php_file,
    );
    let last = first + wanted;
    while let Some(lexeme) = lexer.next()? {
        if lexeme.line >= last {
            break;
        }
        push(&mut out, first, &lexeme);
        if lexeme.col as usize >= MAX_LINE_BYTES
            && let Some(next) = lines.exact(lexeme.line + 1)
        {
            // The rest of a long line: on at the checkpoint after it.
            lexer.seek_line(next.offset, next.line, lines.state(&next));
        }
    }
    Ok(out)
}

/// Adds the token of `lexeme` to its line's list when that line was asked for and has room.
fn push(out: &mut [Vec<Token>], first: u64, lexeme: &Lexeme) {
    let Some(class) = lexeme.class else {
        return;
    };
    let Some(tokens) = lexeme
        .line
        .checked_sub(first)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| out.get_mut(index))
    else {
        return;
    };
    if lexeme.len == 0
        || lexeme.col as usize + lexeme.len as usize > MAX_LINE_BYTES
        || tokens.len() >= MAX_TOKENS_PER_LINE
    {
        return;
    }
    tokens.push(Token {
        start: lexeme.col,
        len: lexeme.len,
        class,
    });
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
