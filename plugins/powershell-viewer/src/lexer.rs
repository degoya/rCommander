//! The PowerShell lexer: one line at a time, in the state the line before left, for both passes
//! (the line index of `open` and the tokens of the lines on screen). It reads the UTF-8 text the
//! Lister shows, so every offset is a byte of it.
//!
//! What reaches from one line into the next is the [`LineState`]: the open frames ([`Frame`]) -
//! strings, here-strings, a block comment, `( )` and `$( )` with the strings inside them - and
//! where the next word stands when a backtick at the end of the line continued it. Inside a
//! string or a comment the next line goes on where this one stopped; anywhere else a line starts
//! a statement.
//!
//! The lexer colours, it does not parse: it knows where a statement starts, where a command's
//! arguments are read and where an expression is, well enough to tell a keyword from an
//! argument, and never fails - a script that is not well formed is coloured as far as it reads.
//! Every decision is on ASCII bytes, so every token sits on character boundaries.

mod code;
mod frames;
mod strings;
mod words;

use frames::{Frame, Frames};

use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The most string openings remembered for `invalid` at the end of the file.
const MAX_OPENS: usize = 64;

/// Where the next word stands, which decides what a bare word is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    /// A statement starts: a keyword, a command's name, or an expression.
    #[default]
    Command,
    /// A command's arguments: a bare word is text, `-Name` a parameter.
    Args,
    /// After a value in an expression: operators, members, `[index]`.
    Expr,
    /// After `function` or `filter`: the name defined.
    FuncName,
    /// After `class` or `enum`: the type defined.
    TypeName,
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) frames: Frames,
    /// Where the line's first word stands: a statement's start, unless the line before ended
    /// inside a string or comment or with a backtick.
    pub(crate) mode: Mode,
}

/// Where a string the file leaves open was opened, and how wide its opening is (`"` 1, `@"` 2).
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

/// `open`'s pass: the strings open at each line's end, where they were opened.
#[derive(Debug, Default)]
pub(crate) struct Opens {
    /// The line being lexed, 0-based; the caller moves it on.
    pub(crate) line: u64,
    open: Vec<QuoteOpen>,
    /// Strings open beyond `open`'s cap.
    lost: u64,
}

impl Opens {
    /// The strings still open at the file's end.
    pub(crate) fn into_unclosed(mut self) -> Vec<QuoteOpen> {
        self.open.shrink_to_fit();
        self.open
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
        if self.lost > 0 {
            self.lost -= 1;
        } else {
            self.open.pop();
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
        run_from: None,
        emit,
    };
    lexer.here_end();
    // A string or comment opened at the line's end still gets its token.
    while lexer.pos < line.len() || lexer.run_from.is_some() {
        match lexer.frames.top() {
            Some(Frame::DQuote | Frame::HereDQ) => lexer.expandable(),
            Some(Frame::SQuote) => lexer.single(),
            Some(Frame::HereSQ) => lexer.literal_line(),
            Some(Frame::Comment) => lexer.block_comment(),
            Some(Frame::Paren | Frame::ParenArgs) | None => lexer.code(),
        }
    }
    let inside = lexer.frames.top().is_some_and(Frame::is_text);
    LineState {
        frames: lexer.frames,
        mode: if lexer.continued || inside {
            lexer.mode
        } else {
            Mode::Command
        },
    }
}

struct Lexer<'a, E: Emit> {
    line: &'a [u8],
    pos: usize,
    frames: Frames,
    mode: Mode,
    /// The line ended in a backtick.
    continued: bool,
    /// Where the string or comment just opened starts: its token takes in the opening.
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

    /// Opens the string or comment `frame` with the `width` bytes at `pos`; its token starts
    /// there. Past the frames kept by kind it is read as code, as [`Frames`] says.
    fn open(&mut self, frame: Frame, width: usize) {
        if self.frames.push(frame) {
            if frame.is_string() {
                self.emit
                    .opened(self.pos, u8::try_from(width).unwrap_or(u8::MAX));
            }
            self.run_from = Some(self.pos);
        }
        self.pos += width;
    }

    /// Closes the innermost frame, a string or comment.
    fn close(&mut self) {
        if self.frames.top().is_some_and(Frame::is_string) {
            self.emit.closed();
        }
        self.frames.pop();
    }

    /// A value was read: an expression goes on after it, a command's arguments stay arguments.
    fn after_value(&mut self) {
        if self.mode != Mode::Args {
            self.mode = Mode::Expr;
        }
    }

    /// `(`, `$(` or `@(` of `width` bytes at `pos`: code inside, a statement starts.
    fn open_paren(&mut self, width: usize) {
        self.push(self.pos, self.pos + width, TokenClass::Punctuation);
        self.pos += width;
        let frame = if self.mode == Mode::Args {
            Frame::ParenArgs
        } else {
            Frame::Paren
        };
        self.frames.push(frame);
        self.mode = Mode::Command;
    }

    /// `)`: the innermost `( )` closes, and what stood before it goes on.
    fn close_paren(&mut self) {
        self.push(self.pos, self.pos + 1, TokenClass::Punctuation);
        self.pos += 1;
        match self.frames.top() {
            Some(Frame::ParenArgs) => {
                self.frames.pop();
                self.mode = Mode::Args;
            }
            Some(Frame::Paren) => {
                self.frames.pop();
                self.mode = Mode::Expr;
            }
            _ => self.mode = Mode::Expr,
        }
    }

    /// A backtick: the next character is taken as it is, or at the end of the line the
    /// statement goes on in the next.
    fn backtick(&mut self) {
        match self.byte(self.pos + 1) {
            None => {
                self.continued = true;
                self.pos = self.line.len();
            }
            Some(next) => self.pos += 1 + words::char_width(next),
        }
        self.pos = self.pos.min(self.line.len());
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
