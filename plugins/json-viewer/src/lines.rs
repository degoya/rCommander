//! The line index: a checkpoint every so many lines or bytes, from which `tokens` reads (or lays
//! out) only the lines it is asked for.
//!
//! A checkpoint is taken at a line start once `every_lines` lines or `every_bytes` bytes of the
//! source have passed since the last one; the byte rule
//! puts one right after a very long line, so no request reads through it to reach the next. The
//! index never holds more than [`MAX_CHECKPOINTS`]: past that every second one is dropped and both
//! distances double, so memory stays bounded whatever the file.

use crate::pretty::LayoutState;

/// The most checkpoints kept; a power of two, so the vector never grows past it.
pub const MAX_CHECKPOINTS: usize = 1 << 16;

const FIRST_EVERY_LINES: u64 = 64;
const FIRST_EVERY_BYTES: u64 = 64 * 1024;

/// Where a line starts: in the source, and in the pretty printer's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Checkpoint {
    /// The line, 0-based.
    pub(crate) line: u64,
    /// As-is: the line's first byte. Pretty: the lexeme whose layout starts the line.
    pub(crate) offset: u64,
    /// Pretty: the printer's state before that lexeme.
    pub(crate) state: LayoutState,
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
                line: 0,
                offset,
                state: LayoutState::default(),
            }],
            every_lines: FIRST_EVERY_LINES,
            every_bytes: FIRST_EVERY_BYTES,
            lines: 0,
        }
    }

    /// Line `line` starts at `offset` with the printer in `state`; lines arrive in order.
    pub(crate) fn line_start(&mut self, line: u64, offset: u64, state: LayoutState) {
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

    /// The checkpoint of exactly `line`, when there is one.
    pub(crate) fn exact(&self, line: u64) -> Option<Checkpoint> {
        let point = self.at_or_before(line);
        (point.line == line).then_some(point)
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.points.capacity() * size_of::<Checkpoint>()
    }
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
