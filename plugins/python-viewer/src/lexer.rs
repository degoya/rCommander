//! The Python lexer: one line at a time, in the state the line before left, for both passes (the
//! line index of `open` and the tokens of the lines on screen). It reads the UTF-8 text the Lister
//! shows, so every offset is a byte of it.
//!
//! What reaches from one line into the next is the [`LineState`]: the open frames ([`Frame`]) -
//! triple-quoted strings, a string continued by a backslash, the replacement fields of f-strings
//! with the strings and brackets inside them, and code brackets - and where the next word stands.
//! A line starts a statement when no bracket is open and the line before did not end in a
//! backslash; only there are decorators and the soft keywords `match`, `case` and `type` read.
//!
//! The lexer colours, it does not parse, and never fails: a file that is not well formed is
//! coloured as far as it reads. f-strings are read as Python 3.12 reads them (PEP 701): a field
//! may hold strings in the same quote and go on over lines. Every decision is on ASCII bytes, so
//! every token sits on character boundaries.

mod code;
mod frames;
mod strings;
mod words;

use frames::{Frame, Frames};

use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The most strings remembered open at once for `invalid` at the end of the file.
const MAX_OPENS: usize = 64;
/// The most strings remembered that a line ended without closing them.
const MAX_FAILED: usize = 1024;

/// Where the next word stands, which decides what a name is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    /// A statement starts: a decorator, a soft keyword.
    #[default]
    Start,
    /// Anywhere else in code.
    Expr,
    /// After `.`: an attribute, never a built-in name.
    Attr,
    /// After `def`: the function defined.
    DefName,
    /// After `class` or the soft keyword `type`: the type defined.
    ClassName,
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) frames: Frames,
    /// Where the line's first word stands: a statement's start, unless a bracket is open or the
    /// line before ended in a backslash.
    pub(crate) mode: Mode,
}

/// Where a string was opened that the file leaves open, or that its line ended without closing:
/// its opening (prefix and quotes, `rb"` 3, `"""` 3) shows as `invalid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QuoteOpen {
    pub(crate) line: u64,
    pub(crate) col: u32,
    pub(crate) width: u8,
}

/// What the lexer reports besides the state: tokens, and the strings it opens and closes.
pub(crate) trait Emit {
    /// The token `start..end` of the line.
    fn token(&mut self, _start: usize, _end: usize, _class: TokenClass) {}
    /// A string opens at byte `col`, its opening `width` bytes wide.
    fn opened(&mut self, _col: usize, _width: u8) {}
    /// The innermost string open closes.
    fn closed(&mut self) {}
    /// The innermost string open ends with its line, unclosed: Python refuses it.
    fn failed(&mut self) {}
}

/// Only the state.
impl Emit for () {}

/// The tokens of one line, at most [`MAX_TOKENS_PER_LINE`].
impl Emit for Vec<Token> {
    fn token(&mut self, start: usize, end: usize, class: TokenClass) {
        let (Ok(at), Ok(len)) = (
            u32::try_from(start),
            u32::try_from(end.saturating_sub(start)),
        ) else {
            return;
        };
        if len > 0 && self.len() < MAX_TOKENS_PER_LINE {
            self.push(Token {
                start: at,
                len,
                class,
            });
        }
    }
}

/// `open`'s pass: the strings open at each line's end, where they were opened, and the strings a
/// line ended without closing.
#[derive(Debug, Default)]
pub(crate) struct Opens {
    /// The line being lexed, 0-based; the caller moves it on.
    pub(crate) line: u64,
    open: Vec<QuoteOpen>,
    /// Strings open beyond `open`'s cap.
    lost: u64,
    failed: Vec<QuoteOpen>,
}

impl Opens {
    /// The strings to show as `invalid`: those a line ended unclosed and those still open at the
    /// file's end, in the order of the file.
    pub(crate) fn into_unclosed(self) -> Vec<QuoteOpen> {
        let mut all = self.failed;
        all.extend(self.open);
        all.sort_by_key(|open| (open.line, open.col));
        all.shrink_to_fit();
        all
    }

    /// Takes the innermost string open off the stack.
    fn pop(&mut self) -> Option<QuoteOpen> {
        if self.lost > 0 {
            self.lost -= 1;
            None
        } else {
            self.open.pop()
        }
    }
}

impl Emit for Opens {
    fn opened(&mut self, col: usize, width: u8) {
        match u32::try_from(col) {
            Ok(col) if self.lost == 0 && self.open.len() < MAX_OPENS => {
                self.open.push(QuoteOpen {
                    line: self.line,
                    col,
                    width,
                });
            }
            _ => self.lost += 1,
        }
    }

    fn closed(&mut self) {
        self.pop();
    }

    fn failed(&mut self) {
        if let Some(open) = self.pop()
            && self.failed.len() < MAX_FAILED
        {
            self.failed.push(open);
        }
    }
}

/// Lexes `line` (UTF-8, without its line break and a `\r` before it) from `state`, reporting to
/// `emit`. Returns the state the next line starts in.
pub(crate) fn lex_line(line: &[u8], state: LineState, emit: &mut impl Emit) -> LineState {
    let mut lexer = Lexer {
        line,
        pos: 0,
        frames: state.frames,
        mode: state.mode,
        continued: false,
        string_continued: false,
        run_from: None,
        emit,
    };
    // A string opened at the line's end still gets its token.
    while lexer.pos < line.len() || lexer.run_from.is_some() {
        match lexer.frames.top() {
            Some(Frame::Str(quote)) => lexer.string(quote),
            Some(Frame::Spec) => lexer.spec(),
            Some(Frame::Field | Frame::Brackets(_)) | None => lexer.code(),
        }
    }
    lexer.line_end()
}

struct Lexer<'a, E: Emit> {
    line: &'a [u8],
    pos: usize,
    frames: Frames,
    mode: Mode,
    /// The line's code ended in a backslash: the statement goes on.
    continued: bool,
    /// The line ended in a backslash inside a one-line string: the string goes on.
    string_continued: bool,
    /// Where the string just opened starts: its token takes in the opening.
    run_from: Option<usize>,
    emit: &'a mut E,
}

impl<E: Emit> Lexer<'_, E> {
    fn byte(&self, at: usize) -> Option<u8> {
        self.line.get(at).copied()
    }

    fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        self.emit.token(start, end, class);
    }

    /// Closes the string on top.
    fn close_string(&mut self) {
        self.emit.closed();
        self.frames.pop();
    }

    /// The state the next line starts in. A one-quote string the line leaves open ends here, as
    /// Python ends it - with an error - unless a backslash continued it.
    fn line_end(mut self) -> LineState {
        if let Some(Frame::Str(quote)) = self.frames.top()
            && !quote.triple
            && !self.string_continued
        {
            self.emit.failed();
            self.frames.pop();
        }
        let mode = if self.frames.depth() == 0 && !self.continued {
            Mode::Start
        } else {
            self.mode
        };
        LineState {
            frames: self.frames,
            mode,
        }
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
