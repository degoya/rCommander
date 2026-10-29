//! The tokens of the lines asked for, for the `highlighted` output.
//!
//! The classifier starts at the nearest checkpoint in the state the checkpoint kept, so a hunk
//! begun lines before is still counted. A line gets at most two tokens, each running to where
//! the line's text ends - at most [`MAX_LINE_BYTES`] of it, on a character boundary:
//!
//! - an added line `number` (green), a removed line `invalid` (red), an unchanged line none;
//! - a hunk header `keyword`, the function context after it `function`;
//! - `diff`, `index`, `---`, `+++`, the modes, renames, copies and binary markers `type-name`;
//! - `\ No newline at end of file` and the text outside the diff `comment`.

use crate::classify::{Kind, classify};
use crate::lines::LineIndex;
use crate::reader::{End, LineReader};
use crate::types::{Failure, Source, Token, TokenClass};
use crate::utf8;

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); a diff's line has two at most.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The bytes of a line that get tokens; the rest of a longer line stays uncoloured.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The bytes of a line read to classify and measure it: four more than [`MAX_LINE_BYTES`], so a
/// character cut at the end never shortens the measured text below it.
pub(crate) const KEEP_BYTES: usize = MAX_LINE_BYTES + 4;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// The tokens of lines `first..first + wanted`, one list per line.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let mut out: Vec<Vec<Token>> = (0..wanted).map(|_| Vec::new()).collect();
    let point = lines.at_or_before(first);
    let mut reader = LineReader::new(src, point.offset, LINE_CHUNK);
    let mut state = point.state;
    let mut line = point.line;
    let mut head = Vec::with_capacity(KEEP_BYTES);
    let last = first + wanted;
    while line < last {
        let Some(end) = reader.next_line(&mut head, KEEP_BYTES)? else {
            break;
        };
        if end == End::Cut {
            // The rest of a long line: on at the checkpoint after it, else read through it.
            match lines.exact(line + 1) {
                Some(next) => reader.seek(next.offset),
                None => reader.skip_line()?,
            }
        }
        let text = content(&head, end);
        let kind = classify(&mut state, text);
        if let Some(slot) = line
            .checked_sub(first)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| out.get_mut(index))
        {
            *slot = line_tokens_of(kind, text, end == End::Cut);
        }
        line += 1;
    }
    Ok(out)
}

/// What the classifier sees of a line: its bytes without the `\r` of a CRLF line end.
pub(crate) fn content(head: &[u8], end: End) -> &[u8] {
    if end == End::Cut {
        return head;
    }
    head.strip_suffix(b"\r").unwrap_or(head)
}

/// The tokens of one line of kind `kind`; `cut`: the line goes on past `text`.
pub(crate) fn line_tokens_of(kind: Kind, text: &[u8], cut: bool) -> Vec<Token> {
    let len = utf8::text_len(text, cut, MAX_LINE_BYTES);
    let whole = |class: TokenClass| {
        if len == 0 {
            Vec::new()
        } else {
            vec![Token {
                start: 0,
                len,
                class,
            }]
        }
    };
    match kind {
        Kind::Unchanged => Vec::new(),
        Kind::Added => whole(TokenClass::Number),
        Kind::Removed => whole(TokenClass::Invalid),
        Kind::DiffStart | Kind::Header(_) => whole(TokenClass::TypeName),
        Kind::ContextHunk | Kind::ContextRange => whole(TokenClass::Keyword),
        Kind::Text | Kind::NoNewline => whole(TokenClass::Comment),
        Kind::HunkHead { end, context } => {
            // Everything before `end` is ASCII: its bytes are the text's.
            let end = u32::try_from(end).unwrap_or(u32::MAX).min(len);
            let context = u32::try_from(context).unwrap_or(u32::MAX).min(len);
            let mut tokens = Vec::with_capacity(2);
            if end > 0 {
                tokens.push(Token {
                    start: 0,
                    len: end,
                    class: TokenClass::Keyword,
                });
            }
            if len > context {
                tokens.push(Token {
                    start: context,
                    len: len - context,
                    class: TokenClass::Function,
                });
            }
            tokens
        }
    }
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
