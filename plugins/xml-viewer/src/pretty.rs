//! The pretty printer behind `format = pretty`: an element a line, two spaces a level, an element
//! holding only text on one line (`<a>text</a>`), a tag's attributes on its line one space apart,
//! the whitespace between markup dropped and the whitespace inside text folded to one space.
//!
//! A construct over several lines - a comment, a CDATA section, a processing instruction, a
//! DOCTYPE, an attribute value - keeps its lines as the file has them: each piece after the first
//! starts a line of its own, unindented, with its own leading whitespace.
//!
//! It is a layout over lexemes, not over elements, so it prints invalid XML too, as well as it
//! can: depth never goes below zero. Its state before a lexeme ([`LayoutState`]) is all it needs to
//! start again there, which is what a pretty checkpoint stores.

use crate::lexer::{Kind, Lexeme, Mode};

/// The deepest indentation printed; deeper levels print at this one, so a file of ten thousand
/// nested elements does not turn into gigabytes of spaces.
pub const MAX_INDENT_LEVELS: u32 = 64;

/// What the start tag laid out last is waiting for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Pending {
    /// Nothing: no start tag is waiting.
    #[default]
    None,
    /// Its `>` was the last thing laid out.
    Open,
    /// Text follows it on its line.
    Inline,
}

/// The printer's state between two lexemes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayoutState {
    depth: u32,
    /// The element of the last start tag, not indented into yet: its end tag may follow inline.
    pending: Pending,
    /// The line ends in text: more text joins it.
    after_text: bool,
    /// Something was laid out: the next line needs a break.
    started: bool,
}

/// What one lexeme becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Piece {
    /// The line ends; the next starts with this many levels of indentation.
    Break(u32),
    /// The line ends; the next starts with the lexeme as the file has it.
    RawBreak,
    /// The lexeme's own bytes.
    Lexeme,
    /// One space.
    Space,
}

/// At most two pieces per lexeme: a break or a space, then the lexeme.
#[derive(Debug, Default)]
pub(crate) struct Pieces {
    items: [Option<Piece>; 2],
}

impl Pieces {
    fn push(&mut self, piece: Piece) {
        if let Some(slot) = self.items.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(piece);
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Piece> + '_ {
        self.items.iter().flatten().copied()
    }
}

impl LayoutState {
    /// Lays out `lexeme`.
    pub(crate) fn feed(&mut self, lexeme: &Lexeme) -> Pieces {
        let mut pieces = Pieces::default();
        let first = !self.started;
        self.started = true;
        let mode = lexeme.state.mode;
        if mode.is_construct() {
            // A construct's next piece: on the line it had, or right after the piece before.
            if lexeme.state.newline {
                pieces.push(Piece::RawBreak);
            }
            pieces.push(Piece::Lexeme);
            return pieces;
        }
        match (mode, lexeme.kind) {
            (_, Kind::Open | Kind::Comment { .. } | Kind::Pi { .. } | Kind::Doctype { .. }) => {
                self.markup(first, &mut pieces);
            }
            (_, Kind::EndOpen) => self.end_tag(first, &mut pieces),
            (_, Kind::Close) => {
                self.pending = Pending::Open;
                pieces.push(Piece::Lexeme);
            }
            (Mode::Content, _) => self.text(first, lexeme.state.spaced, &mut pieces),
            (_, Kind::AttrName | Kind::Invalid(_)) => {
                pieces.push(Piece::Space);
                pieces.push(Piece::Lexeme);
            }
            _ => pieces.push(Piece::Lexeme),
        }
        pieces
    }

    /// A start tag, comment, PI or DOCTYPE: a line of its own, inside the element waiting.
    fn markup(&mut self, first: bool, pieces: &mut Pieces) {
        if self.pending != Pending::None {
            self.depth = self.depth.saturating_add(1);
        }
        self.pending = Pending::None;
        self.after_text = false;
        if !first {
            pieces.push(Piece::Break(self.depth));
        }
        pieces.push(Piece::Lexeme);
    }

    /// An end tag: inline after its start tag or its text alone, else a line of its own a level
    /// out.
    fn end_tag(&mut self, first: bool, pieces: &mut Pieces) {
        let inline = self.pending != Pending::None;
        self.pending = Pending::None;
        self.after_text = false;
        if !inline {
            self.depth = self.depth.saturating_sub(1);
            if !first {
                pieces.push(Piece::Break(self.depth));
            }
        }
        pieces.push(Piece::Lexeme);
    }

    /// Text, an entity or a CDATA section: after its start tag, joined to the text before, or a
    /// line of its own.
    fn text(&mut self, first: bool, spaced: bool, pieces: &mut Pieces) {
        if self.pending == Pending::Open {
            self.pending = Pending::Inline;
        } else if self.after_text {
            if spaced {
                pieces.push(Piece::Space);
            }
        } else if !first {
            pieces.push(Piece::Break(self.depth));
        }
        self.after_text = true;
        pieces.push(Piece::Lexeme);
    }
}

static SPACES: [u8; 2 * MAX_INDENT_LEVELS as usize] = [b' '; 2 * MAX_INDENT_LEVELS as usize];

/// The indentation of a line `depth` levels deep: two spaces a level, at most
/// [`MAX_INDENT_LEVELS`] levels.
pub(crate) fn indentation(depth: u32) -> &'static [u8] {
    let width = depth.min(MAX_INDENT_LEVELS) as usize * 2;
    SPACES.get(..width).unwrap_or(SPACES.as_slice())
}

#[cfg(test)]
#[path = "pretty_tests.rs"]
mod tests;
