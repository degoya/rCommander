//! The line index: a checkpoint every so many lines or bytes, from which `tokens` reads only the
//! lines it is asked for. A checkpoint keeps the lexer's state at its line's start, so the lexer
//! starts there as if it had read everything before.
//!
//! A checkpoint is taken at a line start once `every_lines` lines or `every_bytes` bytes of the
//! source have passed since the last one; the byte rule puts one right after a very long line,
//! so no request reads through it to reach the next. A state's frames are kept apart, one after
//! the other, only as deep as the stack is: most lines start in one or two. The index never
//! holds more than [`MAX_CHECKPOINTS`] checkpoints and [`MAX_FRAMES`] frames: past either every
//! second checkpoint is dropped and both distances double, so memory stays bounded whatever the
//! file.

use crate::lexer::LineState;
use crate::lexer::state::{Frame, PhpSub, Stack};

/// The most checkpoints kept; a power of two, so the vector never grows past it.
pub const MAX_CHECKPOINTS: usize = 1 << 16;
/// The most frames the checkpoints keep, together.
pub const MAX_FRAMES: usize = 1 << 18;

const FIRST_EVERY_LINES: u64 = 64;
const FIRST_EVERY_BYTES: u64 = 64 * 1024;

/// Where a line starts, and what it starts inside of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Checkpoint {
    /// The line, 0-based.
    pub(crate) line: u64,
    /// The line's first byte.
    pub(crate) offset: u64,
    php: Option<PhpSub>,
    /// The state's frames: `depth` of them from `first` in the index's frames.
    first: u32,
    depth: u8,
}

#[derive(Debug)]
pub(crate) struct LineIndex {
    points: Vec<Checkpoint>,
    frames: Vec<Frame>,
    every_lines: u64,
    every_bytes: u64,
    lines: u64,
}

impl LineIndex {
    /// An index whose line 0 starts at `offset` in `state`.
    pub(crate) fn new(offset: u64, state: &LineState) -> Self {
        let mut index = Self {
            points: Vec::with_capacity(1024),
            frames: Vec::with_capacity(1024),
            every_lines: FIRST_EVERY_LINES,
            every_bytes: FIRST_EVERY_BYTES,
            lines: 0,
        };
        index.keep(0, offset, state);
        index
    }

    /// Line `line` starts at `offset` in `state`; lines arrive in order.
    pub(crate) fn line_start(&mut self, line: u64, offset: u64, state: &LineState) {
        let Some(last) = self.points.last() else {
            return;
        };
        if line < last.line + self.every_lines && offset < last.offset + self.every_bytes {
            return;
        }
        if self.frames.len() + state.stack.len() > MAX_FRAMES {
            self.thin();
        }
        self.keep(line, offset, state);
        if self.points.len() >= MAX_CHECKPOINTS {
            self.thin();
        }
    }

    fn keep(&mut self, line: u64, offset: u64, state: &LineState) {
        let frames = state.stack.frames();
        self.points.push(Checkpoint {
            line,
            offset,
            php: state.php,
            first: u32::try_from(self.frames.len()).unwrap_or(u32::MAX),
            depth: u8::try_from(frames.len()).unwrap_or(u8::MAX),
        });
        self.frames.extend_from_slice(frames);
    }

    /// Keeps line 0 and every second checkpoint after it, and their frames; both distances
    /// double.
    fn thin(&mut self) {
        let points = std::mem::take(&mut self.points);
        let frames = std::mem::take(&mut self.frames);
        for (index, point) in points.into_iter().enumerate() {
            if index % 2 == 1 {
                continue;
            }
            let kept = frames_of(&frames, &point);
            self.points.push(Checkpoint {
                first: u32::try_from(self.frames.len()).unwrap_or(u32::MAX),
                ..point
            });
            self.frames.extend_from_slice(kept);
        }
        self.every_lines *= 2;
        self.every_bytes *= 2;
    }

    /// The number of lines, known once the pass is done.
    pub(crate) fn finish(&mut self, lines: u64) {
        self.lines = lines;
        self.points.shrink_to_fit();
        self.frames.shrink_to_fit();
    }

    pub(crate) fn lines(&self) -> u64 {
        self.lines
    }

    /// The last checkpoint at or before `line`.
    pub(crate) fn at_or_before(&self, line: u64) -> Checkpoint {
        let after = self.points.partition_point(|point| point.line <= line);
        let first = self.points.first().copied();
        self.points
            .get(after.saturating_sub(1))
            .copied()
            .or(first)
            .unwrap_or(Checkpoint {
                line: 0,
                offset: 0,
                php: None,
                first: 0,
                depth: 0,
            })
    }

    /// The checkpoint of exactly `line`, when there is one.
    pub(crate) fn exact(&self, line: u64) -> Option<Checkpoint> {
        let point = self.at_or_before(line);
        (point.line == line).then_some(point)
    }

    /// The state a checkpoint keeps.
    pub(crate) fn state(&self, point: &Checkpoint) -> LineState {
        LineState {
            stack: Stack::of(frames_of(&self.frames, point)),
            php: point.php,
        }
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.points.capacity() * size_of::<Checkpoint>()
            + self.frames.capacity() * size_of::<Frame>()
    }
}

/// The frames of `point` in `frames`.
fn frames_of<'a>(frames: &'a [Frame], point: &Checkpoint) -> &'a [Frame] {
    let first = point.first as usize;
    frames
        .get(first..first + usize::from(point.depth))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
