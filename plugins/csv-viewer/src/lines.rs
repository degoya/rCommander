//! The indexes of the open pass: a checkpoint every so many lines (or records) or bytes, from
//! which `tokens` reads only the lines it is asked for and the tree only the records it lists. A
//! line checkpoint keeps the state its line starts in - inside a quoted field, and which column -
//! so a field over several lines keeps its column's colour on each.
//!
//! A checkpoint is taken once `every_count` lines (records) or `every_bytes` bytes of the source
//! have passed since the last one; the byte rule puts one right after a very long line, so no
//! request reads through it to reach the next. An index never holds more than
//! [`MAX_CHECKPOINTS`]: past that every second one is dropped and both distances double, so memory
//! stays bounded whatever the file.

/// The most checkpoints one index keeps; a power of two, so the vector never grows past it.
pub const MAX_CHECKPOINTS: usize = 1 << 16;

const FIRST_EVERY_COUNT: u64 = 64;
const FIRST_EVERY_BYTES: u64 = 64 * 1024;

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    /// Inside a quoted field begun on an earlier line.
    pub(crate) quoted: bool,
    /// The column of that field; 0 when the line starts a record.
    pub(crate) column: u32,
}

/// Where a line (or record) starts, and in what state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Checkpoint<T> {
    /// The line (record), 0-based.
    pub(crate) at: u64,
    /// Its first byte.
    pub(crate) offset: u64,
    pub(crate) state: T,
}

#[derive(Debug)]
pub(crate) struct Index<T> {
    points: Vec<Checkpoint<T>>,
    every_count: u64,
    every_bytes: u64,
    count: u64,
}

/// Lines and the state each starts in.
pub(crate) type LineIndex = Index<LineState>;
/// Records of the file, the header included.
pub(crate) type RecordIndex = Index<()>;

impl<T: Copy + Default> Index<T> {
    /// An index whose first line (record) starts at `offset`.
    pub(crate) fn new(offset: u64) -> Self {
        Self {
            points: vec![Checkpoint {
                at: 0,
                offset,
                state: T::default(),
            }],
            every_count: FIRST_EVERY_COUNT,
            every_bytes: FIRST_EVERY_BYTES,
            count: 0,
        }
    }

    /// Line (record) `at` starts at `offset` in `state`; they arrive in order.
    pub(crate) fn start(&mut self, at: u64, offset: u64, state: T) {
        if at == 0 {
            // The first one moves: the file's first record may follow blank lines.
            if let Some(first) = self.points.first_mut() {
                (first.offset, first.state) = (offset, state);
            }
            return;
        }
        let Some(last) = self.points.last() else {
            return;
        };
        if at < last.at + self.every_count && offset < last.offset + self.every_bytes {
            return;
        }
        self.points.push(Checkpoint { at, offset, state });
        if self.points.len() >= MAX_CHECKPOINTS {
            self.thin();
        }
    }

    /// Keeps the first checkpoint and every second one after it; both distances double.
    fn thin(&mut self) {
        let mut index = 0usize;
        self.points.retain(|_| {
            index += 1;
            index % 2 == 1
        });
        self.every_count *= 2;
        self.every_bytes *= 2;
    }

    /// The number of lines (records), known once the pass is done.
    pub(crate) fn finish(&mut self, count: u64) {
        self.count = count;
        self.points.shrink_to_fit();
    }

    pub(crate) fn count(&self) -> u64 {
        self.count
    }

    /// The last checkpoint at or before `at`.
    pub(crate) fn at_or_before(&self, at: u64) -> Checkpoint<T> {
        let after = self.points.partition_point(|point| point.at <= at);
        self.points
            .get(after.saturating_sub(1))
            .copied()
            .unwrap_or_default()
    }

    /// The checkpoint of exactly `at`, when there is one.
    pub(crate) fn exact(&self, at: u64) -> Option<Checkpoint<T>> {
        let point = self.at_or_before(at);
        (point.at == at).then_some(point)
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.points.capacity() * size_of::<Checkpoint<T>>()
    }
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
