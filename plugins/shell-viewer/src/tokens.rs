//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The lexer starts at the nearest checkpoint in the state the checkpoint kept, so a quote, a
//! substitution or a heredoc begun lines before is still known. A quote the file never closes
//! has its opening marked `invalid`. Every span starts and ends on a character of the text
//! written, so it sits on character boundaries whatever the line holds.

use crate::lexer::{Lexeme, Lexer, QuoteOpen};
use crate::lines::LineIndex;
use crate::reader::Reader;
use crate::types::{Failure, Source, Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The bytes of a line that get tokens; the rest of a longer line stays uncoloured.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// The tokens of lines `first..first + wanted`, one list per line.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    unclosed: &[QuoteOpen],
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let mut out: Vec<Vec<Token>> = (0..wanted).map(|_| Vec::new()).collect();
    let point = lines.at_or_before(first);
    let mut lexer = Lexer::resume(
        Reader::new(src, point.offset, LINE_CHUNK),
        point.line,
        point.state,
    );
    let last = first + wanted;
    while let Some(lexeme) = lexer.next()? {
        if lexeme.line >= last {
            break;
        }
        push(&mut out, first, &lexeme, unclosed);
        if lexeme.col as usize >= MAX_LINE_BYTES
            && let Some(next) = lines.exact(lexeme.line + 1)
        {
            // The rest of a long line: on at the checkpoint after it.
            lexer.seek_line(next.offset, next.line, next.state);
        }
    }
    Ok(out)
}

/// Adds the token of `lexeme` to its line's list when that line was asked for and has room; the
/// opening of a quote left open splits off as `invalid`.
fn push(out: &mut [Vec<Token>], first: u64, lexeme: &Lexeme, unclosed: &[QuoteOpen]) {
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
    if lexeme.len == 0 || lexeme.col as usize + lexeme.len as usize > MAX_LINE_BYTES {
        return;
    }
    let open = unclosed
        .iter()
        .find(|open| open.line == lexeme.line && open.col == lexeme.col);
    let width = open.map_or(0, |open| u32::from(open.width).min(lexeme.len));
    if width > 0 {
        add(tokens, lexeme.col, width, TokenClass::Invalid);
    }
    if lexeme.len > width {
        add(tokens, lexeme.col + width, lexeme.len - width, class);
    }
}

fn add(tokens: &mut Vec<Token>, start: u32, len: u32, class: TokenClass) {
    if tokens.len() < MAX_TOKENS_PER_LINE {
        tokens.push(Token { start, len, class });
    }
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
