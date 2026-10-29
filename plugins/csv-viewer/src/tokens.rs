//! The tokens of the lines asked for, for the `highlighted` output: every column in its own
//! colour, as "Rainbow CSV" shows it in an editor.
//!
//! The classes are the contract's, fixed and language-neutral, so a column takes the eight of
//! [`COLUMN_CLASSES`] in turn (column 9 is `key` again). A field's quotes are its column's colour,
//! a separator is `punctuation`, a broken field - text after its closing quote, or a quote never
//! closed - is `invalid`. A line is lexed from the nearest checkpoint in the state the checkpoint
//! kept, so a quoted field over several lines keeps its column's colour on each. Every span starts
//! and ends at an ASCII byte or the line's end, so it sits on character boundaries.

use crate::encoding::{Encoding, decode_into};
use crate::lines::{LineIndex, LineState};
use crate::machine::{Kind, Machine, Mode};
use crate::reader::Reader;
use crate::types::{Failure, Source, Token, TokenClass};

/// The classes the columns take in turn.
pub const COLUMN_CLASSES: [TokenClass; 8] = [
    TokenClass::Key,
    TokenClass::String,
    TokenClass::Number,
    TokenClass::Keyword,
    TokenClass::TypeName,
    TokenClass::Function,
    TokenClass::Literal,
    TokenClass::Operator,
];
/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The bytes of a line in the file that get tokens; the rest of a longer line stays uncoloured.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// The class of column `column`.
#[must_use]
pub fn column_class(column: u32) -> TokenClass {
    COLUMN_CLASSES[column as usize % COLUMN_CLASSES.len()]
}

/// Lexes line after line, carrying the state from one to the next.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineLexer {
    machine: Machine,
    column: u32,
}

impl LineLexer {
    pub(crate) fn new(sep: u8, state: LineState) -> Self {
        let mode = if state.quoted {
            Mode::Quoted
        } else {
            Mode::RecordStart
        };
        Self {
            machine: Machine::new(sep, mode),
            column: state.column,
        }
    }

    /// The tokens of `line` (its text, without the line break). `capped`: the line goes on after
    /// these bytes, so the field touching the end is left out. `unclosed`: a quoted field still
    /// open at the line's end is the one never closed.
    pub(crate) fn line(&mut self, line: &[u8], capped: bool, unclosed: bool) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut from = 0usize;
        for (index, byte) in line.iter().enumerate() {
            let before = self.machine.mode;
            match self.machine.feed(*byte).kind {
                Kind::Blank => from = index + 1,
                Kind::End { broken, .. } => {
                    push(&mut tokens, from, index, self.class(broken));
                    push(&mut tokens, index, index + 1, TokenClass::Punctuation);
                    from = index + 1;
                    self.column = self.column.saturating_add(1);
                }
                Kind::Content(_) => {
                    if before == Mode::RecordStart {
                        from = index;
                    }
                }
            }
        }
        if !capped {
            let broken = match self.machine.mode {
                Mode::Broken => true,
                Mode::Quoted => unclosed,
                _ => false,
            };
            push(&mut tokens, from, line.len(), self.class(broken));
        }
        tokens
    }

    fn class(&self, broken: bool) -> TokenClass {
        if broken {
            TokenClass::Invalid
        } else {
            column_class(self.column)
        }
    }

    /// Bytes of the line past the cap: they move the state, nothing more.
    pub(crate) fn skip(&mut self, bytes: &[u8]) {
        for byte in bytes {
            if let Kind::End { record: false, .. } = self.machine.feed(*byte).kind {
                self.column = self.column.saturating_add(1);
            }
        }
    }

    /// The line break: the state the next line starts in.
    pub(crate) fn end_line(&mut self) -> LineState {
        if let Kind::End { record: true, .. } = self.machine.feed(b'\n').kind {
            self.column = 0;
        }
        LineState {
            quoted: self.machine.mode == Mode::Quoted,
            column: self.column,
        }
    }
}

/// Adds the span `start..end` when it is not empty and the line has room.
fn push(tokens: &mut Vec<Token>, start: usize, end: usize, class: TokenClass) {
    if end <= start || tokens.len() >= MAX_TOKENS_PER_LINE {
        return;
    }
    let (Ok(start), Ok(len)) = (u32::try_from(start), u32::try_from(end - start)) else {
        return;
    };
    tokens.push(Token { start, len, class });
}

/// How the lines of one document are read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    pub(crate) sep: u8,
    pub(crate) encoding: Encoding,
    pub(crate) unclosed_line: Option<u64>,
}

/// The tokens of lines `first..first + wanted`, one list per line, lexed from the nearest
/// checkpoint.
pub(crate) fn line_tokens<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    layout: Layout,
    first: u64,
    wanted: u64,
) -> Result<Vec<Vec<Token>>, Failure> {
    let point = lines.at_or_before(first);
    let mut reader = Reader::new(src, point.offset, LINE_CHUNK);
    let mut lexer = LineLexer::new(layout.sep, point.state);
    let mut line = point.at;
    // The lines before the first asked for move the state only.
    while line < first {
        if !reader.feed_line(&mut |bytes| lexer.skip(bytes))? {
            return Ok(Vec::new());
        }
        lexer.end_line();
        line += 1;
    }
    let mut out = Vec::new();
    let mut text = Vec::new();
    while line < first + wanted {
        let Some(read) = reader.read_line(MAX_LINE_BYTES)? else {
            break;
        };
        text.clear();
        decode_into(&read.bytes, layout.encoding, &mut text);
        let unclosed = layout.unclosed_line.is_some_and(|open| line >= open);
        out.push(lexer.line(&text, read.capped, unclosed));
        line += 1;
        if read.capped {
            // The rest of a long line: on at the checkpoint after it when there is one.
            match lines.exact(line) {
                Some(next) => {
                    reader.seek(next.offset);
                    lexer = LineLexer::new(layout.sep, next.state);
                }
                None => {
                    reader.feed_line(&mut |bytes| lexer.skip(bytes))?;
                    lexer.end_line();
                }
            }
        } else {
            lexer.end_line();
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
