//! The C-family lexer: one line at a time, in the state the line before left, for both passes
//! (the line index of `open` and the tokens of the lines on screen). It reads the UTF-8 text the
//! Lister shows, so every offset is a byte of it.
//!
//! One lexer serves every language: `//` and `/* */` comments, strings, characters, numbers,
//! operators and names are the family's, and [`Lang`] names what differs - the words, C and
//! C++'s preprocessor and backslash splices, C#'s verbatim, interpolated and raw strings,
//! Java's and Kotlin's text blocks and annotations, Kotlin's templates, Go's raw strings, Rust's
//! lifetimes, raw strings, attributes, macros and nested comments.
//!
//! What reaches from one line into the next is the [`LineState`]: the open frames ([`Frame`]) -
//! block comments, strings over lines, the fields of interpolated strings - and what a backslash
//! carried on, the delimiter of a C++ raw string, and where the next word stands. The lexer
//! colours, it does not parse, and never fails: a file that is not well formed is coloured as far
//! as it reads. It never goes back: every step moves on, and what it looks ahead at is bounded,
//! so the time is linear in the line. Every decision is on ASCII bytes, so every token sits on
//! character boundaries.

mod code;
mod frames;
mod marks;
mod strings;
mod words;

use frames::{Frame, Frames, StrKind};

use crate::lang::Lang;
use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The most strings remembered open at once for `invalid` at the end of the file.
const MAX_OPENS: usize = 64;
/// The most strings remembered that a line ended without closing them.
const MAX_FAILED: usize = 1024;
/// The longest delimiter of a C++ raw string (`R"delim(`), the standard's limit.
pub(crate) const MAX_DELIMITER: usize = 16;

/// Where the next word stands, which decides what a name is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Anywhere in code.
    #[default]
    Expr,
    /// After `.`: a number is a tuple's field, without a fraction.
    Member,
    /// After `fn`, `func`, `fun`, `#define`, an annotation's `@` or an attribute's `#[`: the name
    /// is a function's (or macro's, annotation's, attribute's).
    FunctionName,
    /// After `class`, `struct`, `enum`, `interface`, `trait`, ...: the name is a type's.
    TypeName,
}

/// What a backslash at the end of the line before carried on (C and C++).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Carry {
    #[default]
    None,
    /// A directive goes on: the line is code, a `#` in it no new directive.
    Directive,
    /// The text of `#error` or `#warning` goes on, uncoloured.
    DirectiveText,
    /// A `//` comment goes on.
    LineComment,
}

/// The delimiter of the C++ raw string open, `delim` of `R"delim(`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Delimiter {
    bytes: [u8; MAX_DELIMITER],
    len: u8,
}

impl Delimiter {
    pub(crate) fn new(delimiter: &[u8]) -> Self {
        let mut bytes = [0; MAX_DELIMITER];
        let len = delimiter.len().min(MAX_DELIMITER);
        bytes[..len].copy_from_slice(&delimiter[..len]);
        Self {
            bytes,
            len: u8::try_from(len).unwrap_or(0),
        }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) frames: Frames,
    pub(crate) mode: Mode,
    pub(crate) carry: Carry,
    pub(crate) delimiter: Delimiter,
}

/// Where a string was opened that the file leaves open, or that its line ended without closing:
/// its opening (prefix and quotes, `R"x(` 4, `$@"` 3) shows as `invalid`.
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
    /// The innermost string open ends with its line, unclosed: the compiler refuses it.
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

/// Lexes `line` of a `lang` file (UTF-8, without its line break and a `\r` before it) from
/// `state`, reporting to `emit`. Returns the state the next line starts in.
pub(crate) fn lex_line(
    lang: Lang,
    line: &[u8],
    state: LineState,
    emit: &mut impl Emit,
) -> LineState {
    // A backslash carried a `//` comment or `#error`'s text into this line: all of it is that.
    if matches!(state.carry, Carry::LineComment | Carry::DirectiveText) {
        if state.carry == Carry::LineComment {
            emit.token(0, line.len(), TokenClass::Comment);
        }
        let spliced = lang.splices() && line.last() == Some(&b'\\');
        return LineState {
            carry: if spliced { state.carry } else { Carry::None },
            ..state
        };
    }
    let directive = state.carry == Carry::Directive;
    let mut lexer = Lexer {
        lang,
        line,
        pos: 0,
        frames: state.frames,
        mode: state.mode,
        delimiter: state.delimiter,
        line_start: !directive && state.frames.depth() == 0,
        directive,
        carry: Carry::None,
        string_continued: false,
        run_from: None,
        emit,
    };
    // A string or comment opened at the line's end still gets its token.
    while lexer.pos < line.len() || lexer.run_from.is_some() {
        match lexer.frames.top() {
            Some(Frame::Comment(depth)) => lexer.block_comment(depth),
            Some(Frame::Str(open)) => lexer.string(open),
            Some(Frame::Spec(close)) => lexer.spec(close),
            Some(Frame::Field { .. }) | None => lexer.code(),
        }
    }
    lexer.line_end()
}

struct Lexer<'a, E: Emit> {
    lang: Lang,
    line: &'a [u8],
    pos: usize,
    frames: Frames,
    mode: Mode,
    delimiter: Delimiter,
    /// Only blanks so far on a line that starts in code: a `#` here heads a directive.
    line_start: bool,
    /// The line is (the rest of) a directive.
    directive: bool,
    /// What the line's end carries into the next.
    carry: Carry,
    /// The line ended in a backslash inside a one-line string: the string goes on (C, C++).
    string_continued: bool,
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

    /// The line from `start` to its end is one token of `class`, and the lexer is done with it.
    fn rest_of_line(&mut self, start: usize, class: TokenClass) {
        self.push(start, self.line.len(), class);
        self.pos = self.line.len();
    }

    /// A block comment's text, from the line's start or its opening, up to its closing `*/` -
    /// the last of them where comments nest - or the line's end.
    fn block_comment(&mut self, depth: u8) {
        let start = self.run_from.take().unwrap_or(self.pos);
        let nests = self.lang.nested_comments();
        let mut depth = depth;
        let mut at = self.pos;
        while at < self.line.len() {
            let pair = self.line.get(at..at + 2);
            if pair == Some(b"*/") {
                at += 2;
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    self.push(start, at, TokenClass::Comment);
                    self.pos = at;
                    self.frames.pop();
                    return;
                }
            } else if nests && pair == Some(b"/*") {
                at += 2;
                depth = depth.saturating_add(1);
            } else {
                at += 1;
            }
        }
        self.frames.set_top(Frame::Comment(depth));
        self.rest_of_line(start, TokenClass::Comment);
    }

    /// The state the next line starts in. A one-line string the line leaves open ends here, with
    /// the compiler's error, unless the language lets it go on.
    fn line_end(mut self) -> LineState {
        if let Some(Frame::Str(open)) = self.frames.top()
            && open.kind == StrKind::Quoted
            && !self.lang.multiline_quoted()
            && !self.string_continued
        {
            self.emit.failed();
            self.frames.pop();
        }
        LineState {
            frames: self.frames,
            mode: self.mode,
            carry: self.carry,
            delimiter: self.delimiter,
        }
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
