//! The line index: a checkpoint every so many lines or bytes, from which `tokens` and the tree read
//! only the lines they need. A checkpoint keeps the lexer's state at its line's start and the
//! number of entries (key lines) before it, so the lexer starts there as if it had read
//! everything before, and the tree finds its `n`th entry without counting from the top.
//!
//! The batch viewer's index with the entry count added: a checkpoint is taken at a line start once
//! `every_lines` lines or `every_bytes` bytes of the source have passed since the last one; past
//! [`MAX_CHECKPOINTS`] every second one is dropped and both distances double, so memory stays
//! bounded whatever the file.

use crate::lexer::LineState;

/// The most checkpoints kept; a power of two, so the vector never grows past it.
pub const MAX_CHECKPOINTS: usize = 1 << 16;

const FIRST_EVERY_LINES: u64 = 64;
const FIRST_EVERY_BYTES: u64 = 64 * 1024;

/// Where a line starts, what it starts inside of, and how many entries come before it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Checkpoint {
    /// The line, 0-based.
    pub(crate) line: u64,
    /// The line's first byte.
    pub(crate) offset: u64,
    pub(crate) state: LineState,
    /// The entries on the lines before.
    pub(crate) entries: u64,
}

#[derive(Debug)]
pub(crate) struct LineIndex {
    points: Vec<Checkpoint>,
    every_lines: u64,
    every_bytes: u64,
    lines: u64,
}

impl LineIndex {
    /// An index whose line 0 starts at `offset`.
    pub(crate) fn new(offset: u64) -> Self {
        Self {
            points: vec![Checkpoint {
                offset,
                ..Checkpoint::default()
            }],
            every_lines: FIRST_EVERY_LINES,
            every_bytes: FIRST_EVERY_BYTES,
            lines: 0,
        }
    }

    /// Line `line` starts at `offset` in `state`, after `entries` entries; lines arrive in order.
    pub(crate) fn line_start(&mut self, line: u64, offset: u64, state: LineState, entries: u64) {
        let Some(last) = self.points.last() else {
            return;
        };
        if line < last.line + self.every_lines && offset < last.offset + self.every_bytes {
            return;
        }
        self.points.push(Checkpoint {
            line,
            offset,
            state,
            entries,
        });
        if self.points.len() >= MAX_CHECKPOINTS {
            self.thin();
        }
    }

    /// Keeps line 0 and every second checkpoint after it; both distances double.
    fn thin(&mut self) {
        let mut index = 0usize;
        self.points.retain(|_| {
            index += 1;
            index % 2 == 1
        });
        self.every_lines *= 2;
        self.every_bytes *= 2;
    }

    /// The number of lines, known once the pass is done.
    pub(crate) fn finish(&mut self, lines: u64) {
        self.lines = lines;
        self.points.shrink_to_fit();
    }

    pub(crate) fn lines(&self) -> u64 {
        self.lines
    }

    /// The last checkpoint at or before `line`.
    pub(crate) fn at_or_before(&self, line: u64) -> Checkpoint {
        let after = self.points.partition_point(|point| point.line <= line);
        self.points
            .get(after.saturating_sub(1))
            .copied()
            .unwrap_or_default()
    }

    /// The last checkpoint at or before the line of entry `entry` (0-based).
    pub(crate) fn before_entry(&self, entry: u64) -> Checkpoint {
        let after = self.points.partition_point(|point| point.entries <= entry);
        self.points
            .get(after.saturating_sub(1))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.points.capacity() * size_of::<Checkpoint>()
    }
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
