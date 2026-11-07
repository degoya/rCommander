//! A lexer's place in its line, and where it reports to.

use super::Emit;
use crate::types::TokenClass;

pub(crate) struct Cursor<'a, E: Emit> {
    pub(crate) line: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) emit: &'a mut E,
}

impl<'a, E: Emit> Cursor<'a, E> {
    pub(crate) fn new(line: &'a [u8], emit: &'a mut E) -> Self {
        Self { line, pos: 0, emit }
    }

    /// The byte at `at`, if the line has one there.
    pub(crate) fn byte(&self, at: usize) -> Option<u8> {
        self.line.get(at).copied()
    }

    /// The line's length.
    pub(crate) fn end(&self) -> usize {
        self.line.len()
    }

    pub(crate) fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        self.emit.token(start, end, class);
    }

    /// The rest of the line is one token of `class`.
    pub(crate) fn rest(&mut self, class: TokenClass) {
        self.push(self.pos, self.line.len(), class);
        self.pos = self.line.len();
    }

    /// `start..end` is one token of `class`, and the cursor moves past it.
    pub(crate) fn take(&mut self, start: usize, end: usize, class: TokenClass) {
        self.push(start, end, class);
        self.pos = end;
    }

    pub(crate) fn opened(&mut self, col: usize, width: usize) {
        self.emit
            .opened(col, u8::try_from(width).unwrap_or(u8::MAX));
    }

    pub(crate) fn closed(&mut self) {
        self.emit.closed();
    }

    pub(crate) fn failed(&mut self) {
        self.emit.failed();
    }

    /// The end of the blanks from `at`.
    pub(crate) fn blank_end(&self, at: usize) -> usize {
        let mut end = at;
        while matches!(self.byte(end), Some(b' ' | b'\t')) {
            end += 1;
        }
        end
    }

    /// The first `byte` from `from` on, at most `limit` bytes on.
    pub(crate) fn find(&self, from: usize, byte: u8, limit: usize) -> Option<usize> {
        let end = from.saturating_add(limit).min(self.line.len());
        (from..end).find(|&at| self.line[at] == byte)
    }
}
