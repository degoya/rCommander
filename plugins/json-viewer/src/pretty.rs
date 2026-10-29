//! The pretty printer behind `format = pretty`: two spaces per level, a member or element per
//! line, `{}` and `[]` kept together, `": "` after a key.
//!
//! It is a layout over lexemes, not over values, so it prints invalid JSON too, as well as it
//! can: depth never goes below zero and two values in a row get a line each rather than running
//! together. Its state before a lexeme ([`LayoutState`]) is all it needs to start again there,
//! which is what a pretty checkpoint stores.

use crate::lexer::{Kind, Lexeme};

/// The deepest indentation printed; deeper levels print at this one, so a file of ten thousand
/// `[` does not turn into gigabytes of spaces.
pub const MAX_INDENT_LEVELS: u32 = 64;

/// The printer's state between two lexemes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayoutState {
    depth: u32,
    /// After `{` or `[`: the next lexeme decides between `{}` and a new line.
    pending_open: bool,
    /// After a value or a close: another value starts a line of its own.
    after_value: bool,
}

/// What one lexeme becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Piece {
    /// The line ends; the next starts with this many levels of indentation.
    Break(u32),
    /// The lexeme's own bytes.
    Lexeme,
    /// One space, after a colon.
    Space,
}

/// At most three pieces per lexeme: a break, the lexeme, a break or a space.
#[derive(Debug, Default)]
pub(crate) struct Pieces {
    items: [Option<Piece>; 3],
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
        let kind = lexeme.kind;
        if self.pending_open {
            self.pending_open = false;
            if kind.is_close() {
                pieces.push(Piece::Lexeme);
                self.after_value = true;
                return pieces;
            }
            self.depth = self.depth.saturating_add(1);
            pieces.push(Piece::Break(self.depth));
            self.after_value = false;
        }
        match kind {
            Kind::BeginObject | Kind::BeginArray => {
                if self.after_value {
                    pieces.push(Piece::Break(self.depth));
                }
                pieces.push(Piece::Lexeme);
                self.pending_open = true;
                self.after_value = false;
            }
            Kind::EndObject | Kind::EndArray => {
                self.depth = self.depth.saturating_sub(1);
                pieces.push(Piece::Break(self.depth));
                pieces.push(Piece::Lexeme);
                self.after_value = true;
            }
            Kind::Comma => {
                pieces.push(Piece::Lexeme);
                pieces.push(Piece::Break(self.depth));
                self.after_value = false;
            }
            Kind::Colon => {
                pieces.push(Piece::Lexeme);
                pieces.push(Piece::Space);
                self.after_value = false;
            }
            Kind::String | Kind::Number | Kind::Literal | Kind::Invalid(_) => {
                if self.after_value {
                    pieces.push(Piece::Break(self.depth));
                }
                pieces.push(Piece::Lexeme);
                self.after_value = true;
            }
        }
        pieces
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
