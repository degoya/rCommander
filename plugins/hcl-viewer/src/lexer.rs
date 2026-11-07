//! The HCL lexer: one line at a time, in the state the line before left, for both passes (the
//! line index of `open` and the tokens of the lines on screen). It reads the UTF-8 text the Lister
//! shows, so every offset is a byte of it.
//!
//! HCL's native syntax (Terraform's `.tf` and `.tfvars`, Packer's, Nomad's and every other
//! `.hcl`): `#`, `//` and `/* */` comments, blocks (`resource "aws_instance" "web" {`),
//! attributes (`ami = "..."`), quoted templates `"..."` with `${ ... }` interpolations and
//! `%{ ... }` directives, heredocs (`<<EOT` and `<<-EOT` up to the line that holds only `EOT`),
//! numbers, operators, function calls and the type constraints of a variable.
//!
//! What reaches from one line into the next is the [`LineState`]: the open frames ([`Frame`]) -
//! a block comment, a heredoc, a template sequence and the quoted template around it - and the
//! marker that closes the heredoc open. A quoted template ends with its line; one its line leaves
//! open shows its opening quote as `invalid`. The lexer colours, it does not parse, and never
//! fails: a file that is not well formed is coloured as far as it reads. It never goes back, and
//! what it looks ahead at is bounded by the line, so the time is linear in the line. Every
//! decision is on ASCII bytes, so every token sits on character boundaries.

mod code;
mod words;

use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;
/// The longest heredoc marker kept across lines; a longer `<<NAME` opens no heredoc.
pub(crate) const MAX_MARKER: usize = 32;
/// The frames a stack holds; a string or comment opened past them takes the rest of its line,
/// and a template sequence past them stays text. No file nests templates eight deep.
pub(crate) const SLOTS: usize = 8;

/// A construct that goes on until its closer, possibly over several lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// `/* ... */`.
    Comment,
    /// A quoted template `"..."`: closed on its line, else left open there.
    Quoted,
    /// A heredoc's lines, up to the line that holds only its marker.
    Heredoc,
    /// A template sequence, `${ ... }` or `%{ ... }`: code up to its `}`, `depth` braces opened
    /// inside it.
    Sequence { depth: u8 },
}

/// What an unused slot holds, so two stacks with the same frames compare equal.
const EMPTY: Frame = Frame::Comment;

/// The open frames, innermost on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Frames {
    slots: [Frame; SLOTS],
    depth: u8,
}

impl Default for Frames {
    fn default() -> Self {
        Self {
            slots: [EMPTY; SLOTS],
            depth: 0,
        }
    }
}

impl Frames {
    pub(crate) fn depth(self) -> usize {
        usize::from(self.depth)
    }

    pub(crate) fn top(self) -> Option<Frame> {
        self.depth().checked_sub(1).map(|top| self.slots[top])
    }

    /// Pushes `frame`; false when no slot is left.
    pub(crate) fn push(&mut self, frame: Frame) -> bool {
        if self.depth() >= SLOTS {
            return false;
        }
        self.slots[self.depth()] = frame;
        self.depth += 1;
        true
    }

    pub(crate) fn pop(&mut self) {
        if let Some(top) = self.depth().checked_sub(1) {
            self.slots[top] = EMPTY;
            self.depth -= 1;
        }
    }

    /// Replaces the top frame.
    pub(crate) fn set_top(&mut self, frame: Frame) {
        if let Some(top) = self.depth().checked_sub(1) {
            self.slots[top] = frame;
        }
    }

    /// A heredoc is open somewhere in the stack: the one marker the state keeps is taken.
    pub(crate) fn has_heredoc(self) -> bool {
        self.slots[..self.depth()].contains(&Frame::Heredoc)
    }
}

/// The marker of the heredoc open, `EOT` of `<<EOT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Marker {
    len: u8,
    bytes: [u8; MAX_MARKER],
}

impl Marker {
    /// The marker `bytes`, when it fits.
    pub(crate) fn new(bytes: &[u8]) -> Option<Self> {
        let mut marker = Self {
            len: u8::try_from(bytes.len()).ok()?,
            bytes: [0; MAX_MARKER],
        };
        marker.bytes.get_mut(..bytes.len())?.copy_from_slice(bytes);
        Some(marker)
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or_default()
    }
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) frames: Frames,
    /// The marker of the heredoc open; empty when none is.
    pub(crate) marker: Marker,
}

/// What the lexer reports besides the state: its tokens.
pub(crate) trait Emit {
    /// The token `start..end` of the line.
    fn token(&mut self, _start: usize, _end: usize, _class: TokenClass) {}
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

/// Lexes `line` (UTF-8, without its line break and a `\r` before it) from `state`, reporting to
/// `emit`. Returns the state the next line starts in.
pub(crate) fn lex_line(line: &[u8], state: LineState, emit: &mut impl Emit) -> LineState {
    let mut lexer = Lexer {
        line,
        pos: 0,
        frames: state.frames,
        marker: state.marker,
        first: state.frames.depth() == 0,
        emit,
    };
    if lexer.frames.top() == Some(Frame::Heredoc) && lexer.closes_heredoc() {
        return lexer.line_end();
    }
    while lexer.pos < line.len() {
        match lexer.frames.top() {
            None | Some(Frame::Sequence { .. }) => lexer.code(),
            Some(Frame::Comment) => lexer.block_comment(lexer.pos),
            Some(Frame::Quoted) => lexer.template(lexer.pos, true, false),
            Some(Frame::Heredoc) => lexer.template(lexer.pos, false, false),
        }
    }
    lexer.line_end()
}

struct Lexer<'a, E: Emit> {
    line: &'a [u8],
    pos: usize,
    frames: Frames,
    marker: Marker,
    /// Nothing but blanks so far on a line that starts outside every frame: a word here may head
    /// a block.
    first: bool,
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

    /// A block comment from `start`, `pos` inside it: on to its `*/`, or the line's end.
    fn block_comment(&mut self, start: usize) {
        let rest = self.line.get(self.pos..).unwrap_or_default();
        match rest.windows(2).position(|pair| pair == b"*/") {
            Some(close) => {
                self.pos += close + 2;
                self.push(start, self.pos, TokenClass::Comment);
                self.frames.pop();
            }
            None => self.rest_of_line(start, TokenClass::Comment),
        }
    }

    /// The text of the template on top - a quoted one (`quoted`) or a heredoc - from `start`, its
    /// opening quote when `opening`, `pos` inside it: on past its closing quote, to the start of
    /// a template sequence, or to the line's end. `$${` and `%%{` are text; a backslash escapes
    /// in a quoted template only.
    fn template(&mut self, start: usize, quoted: bool, opening: bool) {
        while let Some(byte) = self.byte(self.pos) {
            let next = self.byte(self.pos + 1);
            match byte {
                b'\\' if quoted => self.pos = (self.pos + 2).min(self.line.len()),
                b'$' | b'%' if next == Some(byte) && self.byte(self.pos + 2) == Some(b'{') => {
                    self.pos += 3;
                }
                b'$' | b'%' if next == Some(b'{') => {
                    if !self.frames.push(Frame::Sequence { depth: 0 }) {
                        self.pos += 2;
                        continue;
                    }
                    self.push(start, self.pos, TokenClass::String);
                    let open = self.pos;
                    self.pos += 2;
                    if self.byte(self.pos) == Some(b'~') {
                        self.pos += 1;
                    }
                    self.push(open, self.pos, TokenClass::Punctuation);
                    return;
                }
                b'"' if quoted => {
                    self.pos += 1;
                    self.push(start, self.pos, TokenClass::String);
                    self.frames.pop();
                    return;
                }
                _ => self.pos += 1,
            }
        }
        // The line ends inside the text: a quoted template it opened stays open, which HCL
        // refuses.
        if quoted && opening {
            self.push(start, start + 1, TokenClass::Invalid);
            self.push(start + 1, self.pos, TokenClass::String);
        } else {
            self.push(start, self.pos, TokenClass::String);
        }
    }

    /// Whether the line, which starts inside a heredoc, holds only its marker (blanks around it
    /// allowed): the marker is coloured and the heredoc closes.
    fn closes_heredoc(&mut self) -> bool {
        let marker = self.marker;
        let blank = |byte: u8| matches!(byte, b' ' | b'\t' | b'\r');
        let from = self.line.iter().take_while(|byte| blank(**byte)).count();
        let to = self.line.len()
            - self
                .line
                .iter()
                .rev()
                .take_while(|byte| blank(**byte))
                .count();
        if from >= to || self.line.get(from..to) != Some(marker.as_bytes()) {
            return false;
        }
        self.push(from, to, TokenClass::Keyword);
        self.frames.pop();
        self.marker = Marker::default();
        self.pos = self.line.len();
        true
    }

    /// The state the next line starts in. A quoted template the line leaves open ends here.
    fn line_end(mut self) -> LineState {
        if self.frames.top() == Some(Frame::Quoted) {
            self.frames.pop();
        }
        LineState {
            frames: self.frames,
            marker: self.marker,
        }
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
