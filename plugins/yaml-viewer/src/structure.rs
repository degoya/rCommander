//! The structure `open` learns in its one pass: whether the YAML is well formed and where it
//! first is not, how many documents there are, and an index that lets the tree list any
//! collection's children without reading what lies between.
//!
//! Block collections nest by indentation: a key or `-` at the column of the collection's first
//! child is its next child, one deeper belongs to that child's value, one less indented closes
//! it. A sequence may sit at its key's column (an indentless sequence). A key or item where no
//! collection takes it, or a tab in the indentation, is the first error.
//!
//! The index is the JSON viewer's: an [`Entry`] for every collection or value of at least
//! `min_size` bytes and every collection of more than [`PAGE`] children: its end (the start of
//! the line after it), so a walk jumps over it, its child count, and a mark - the offset of a
//! child - every `mark_step` children, so a page in the middle of a long list starts near its
//! first child. The documents of the stream are counted and marked the same way. The index never
//! takes more than [`MAX_INDEX_BYTES`]: past that `min_size` doubles and the smaller entries go.

use crate::events::{Event, EventKind};
use crate::lexer::LineState;

/// Children per page of the tree: a collection with more is shown as ranges.
pub const PAGE: u64 = 1000;
/// The most memory the index takes.
pub const MAX_INDEX_BYTES: usize = 8 << 20;
/// The deepest nesting of block collections accepted.
pub const MAX_DEPTH: usize = 512;

const FIRST_MIN_SIZE: u64 = 4096;
/// The most marks one collection keeps; past that every second goes and the step doubles.
const MAX_MARKS: usize = 4096;

/// Where the YAML first stops being well formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub offset: u64,
    /// 1-based.
    pub line: u64,
    /// 1-based, in characters.
    pub column: u64,
    pub message: &'static str,
}

/// One indexed collection or value; a collection's `start` is its first child's.
#[derive(Debug, Default)]
pub(crate) struct Entry {
    pub(crate) start: u64,
    pub(crate) end: u64,
    /// Children of a collection; 0 for a value.
    pub(crate) count: u64,
    pub(crate) mark_step: u64,
    /// The offset of child `(i + 1) * mark_step`.
    pub(crate) marks: Box<[u64]>,
}

impl Entry {
    /// The nearest mark at or before child `index`: its offset and child index.
    pub(crate) fn mark(&self, index: u64) -> Option<(u64, u64)> {
        let slot = usize::try_from(index / self.mark_step.max(1))
            .ok()?
            .checked_sub(1)?;
        let offset = *self.marks.get(slot)?;
        Some((offset, (slot as u64 + 1) * self.mark_step))
    }
}

#[derive(Debug, Default)]
pub(crate) struct Structure {
    /// Sorted by `start` once the pass is done.
    entries: Vec<Entry>,
    entry_bytes: usize,
    min_size: u64,
    /// Where the text starts: after a byte-order mark.
    pub(crate) start: u64,
    /// The documents: their count and every `mark_step`-th one's start.
    pub(crate) documents: Entry,
    pub(crate) error: Option<ParseError>,
    /// Where the well-formed part ends: the error's offset, or the end of the file.
    pub(crate) end: u64,
}

impl Structure {
    /// The entry of the collection or value starting at `start`.
    pub(crate) fn entry(&self, start: u64) -> Option<&Entry> {
        self.entries
            .binary_search_by_key(&start, |entry| entry.start)
            .ok()
            .and_then(|index| self.entries.get(index))
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<Entry>()
            + self.entry_bytes
            + self.documents.marks.len() * size_of::<u64>()
    }

    fn record(&mut self, entry: Entry) {
        if entry.end.saturating_sub(entry.start) < self.min_size && entry.count <= PAGE {
            return;
        }
        self.entry_bytes += entry.marks.len() * size_of::<u64>();
        self.entries.push(entry);
        while self.worst_bytes() > MAX_INDEX_BYTES && !self.entries.is_empty() {
            self.min_size *= 2;
            let min_size = self.min_size;
            self.entries
                .retain(|entry| entry.end.saturating_sub(entry.start) >= min_size);
            self.entry_bytes = self
                .entries
                .iter()
                .map(|entry| entry.marks.len() * size_of::<u64>())
                .sum();
        }
    }

    /// The memory the entries could take once the vector grows again: it doubles when full, so
    /// twice what is in it.
    fn worst_bytes(&self) -> usize {
        2 * self.entries.len() * size_of::<Entry>() + self.entry_bytes
    }
}

/// Children counted, with a mark every `step` of them.
#[derive(Debug)]
struct Counter {
    count: u64,
    step: u64,
    marks: Vec<u64>,
}

impl Counter {
    fn new() -> Self {
        Self {
            count: 0,
            step: PAGE,
            marks: Vec::new(),
        }
    }

    /// A child starts at `offset`.
    fn child(&mut self, offset: u64) {
        let index = self.count;
        self.count += 1;
        if index == 0 || !index.is_multiple_of(self.step) {
            return;
        }
        self.marks.push(offset);
        if self.marks.len() >= MAX_MARKS {
            // Keep the marks of the doubled step: every second one.
            let mut position = 0usize;
            self.marks.retain(|_| {
                position += 1;
                position.is_multiple_of(2)
            });
            self.step *= 2;
        }
    }

    fn entry(self, start: u64, end: u64) -> Entry {
        Entry {
            start,
            end,
            count: self.count,
            mark_step: self.step,
            marks: self.marks.into_boxed_slice(),
        }
    }
}

/// The value of a collection's last child, or of a document's root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    /// Nothing yet: a deeper key, item or value is it.
    Open,
    /// A scalar, alias or flow collection: deeper lines may only continue a plain scalar.
    Leaf,
    /// A collection: once it closed, nothing deeper may follow.
    Nested,
}

#[derive(Debug)]
struct Frame {
    start: u64,
    col: u32,
    seq: bool,
    /// A sequence at its key's column, which the next key at that column closes.
    indentless: bool,
    children: Counter,
    value: Value,
}

/// The pass of `open`, fed one event at a time.
pub(crate) struct Builder {
    frames: Vec<Frame>,
    documents: Counter,
    in_document: bool,
    root: Value,
    /// A value that may be large: its start, until the line after it starts.
    leaf: Option<u64>,
    structure: Structure,
}

impl Builder {
    /// A builder for a text starting at `start`.
    pub(crate) fn new(start: u64) -> Self {
        Self {
            frames: Vec::new(),
            documents: Counter::new(),
            in_document: false,
            root: Value::Open,
            leaf: None,
            structure: Structure {
                min_size: FIRST_MIN_SIZE,
                start,
                ..Structure::default()
            },
        }
    }

    pub(crate) fn feed(&mut self, event: &Event) {
        if self.structure.error.is_some() {
            return;
        }
        match event.kind {
            EventKind::Invalid(message) => return self.fail(event, message),
            EventKind::DocStart => {
                self.end_document(event.line_start);
                self.documents.child(event.start);
                self.in_document = true;
                return;
            }
            EventKind::DocEnd => {
                self.end_document(event.line_start);
                self.in_document = false;
                return;
            }
            EventKind::Key | EventKind::Item | EventKind::Value => {}
        }
        if !self.in_document {
            self.documents.child(event.line_start);
            self.in_document = true;
        }
        if self.continues(event) {
            return;
        }
        if event.first_on_line {
            self.end_leaf(event.line_start);
            self.close_above(event);
        }
        self.place(event);
    }

    /// Whether `event` is a further line of a plain scalar.
    fn continues(&self, event: &Event) -> bool {
        if event.kind != EventKind::Value || !event.first_on_line {
            return false;
        }
        match self.frames.last() {
            Some(top) => event.col > top.col && top.value == Value::Leaf,
            None => self.root == Value::Leaf,
        }
    }

    /// Closes the collections a line at `event`'s column leaves.
    fn close_above(&mut self, event: &Event) {
        while let Some(top) = self.frames.last() {
            let closes = top.col > event.col
                || (top.col == event.col && top.indentless && event.kind != EventKind::Item);
            if !closes {
                break;
            }
            if let Some(frame) = self.frames.pop() {
                self.record_frame(frame, event.line_start);
            }
        }
    }

    fn place(&mut self, event: &Event) {
        let Some(top) = self.frames.last_mut() else {
            return self.place_root(event);
        };
        let (col, seq, value) = (top.col, top.seq, top.value);
        if event.col == col {
            match (seq, event.kind) {
                (false, EventKind::Key) | (true, EventKind::Item) => {
                    top.children.child(event.start);
                    top.value = Value::Open;
                }
                (false, EventKind::Item) if value == Value::Open => {
                    top.value = Value::Nested;
                    self.push(event, true, true);
                }
                (false, _) => self.fail(event, "expected a key at this indentation"),
                (true, _) => self.fail(event, "expected '- ' at this indentation"),
            }
            return;
        }
        if event.col < col || value != Value::Open {
            return self.fail(event, "bad indentation");
        }
        match event.kind {
            EventKind::Key | EventKind::Item => {
                top.value = Value::Nested;
                self.push(event, event.kind == EventKind::Item, false);
            }
            _ => {
                top.value = Value::Leaf;
                self.leaf = Some(event.start);
            }
        }
    }

    /// An event outside every collection: the document's root.
    fn place_root(&mut self, event: &Event) {
        if self.root != Value::Open {
            return self.fail(event, "bad indentation");
        }
        match event.kind {
            EventKind::Key | EventKind::Item => {
                self.root = Value::Nested;
                self.push(event, event.kind == EventKind::Item, false);
            }
            _ => {
                self.root = Value::Leaf;
                self.leaf = Some(event.start);
            }
        }
    }

    /// Opens the collection whose first child `event` is.
    fn push(&mut self, event: &Event, seq: bool, indentless: bool) {
        if self.frames.len() >= MAX_DEPTH {
            return self.fail(event, "nesting too deep");
        }
        let mut children = Counter::new();
        children.child(event.start);
        self.frames.push(Frame {
            start: event.start,
            col: event.col,
            seq,
            indentless,
            children,
            value: Value::Open,
        });
    }

    fn end_leaf(&mut self, end: u64) {
        if let Some(start) = self.leaf.take()
            && end > start
        {
            self.structure.record(Entry {
                start,
                end,
                count: 0,
                mark_step: PAGE,
                marks: Box::default(),
            });
        }
    }

    fn record_frame(&mut self, frame: Frame, end: u64) {
        let entry = frame.children.entry(frame.start, end);
        self.structure.record(entry);
    }

    /// Closes everything open at `end`: a document marker, an error or the end of the text.
    fn end_document(&mut self, end: u64) {
        self.end_leaf(end);
        while let Some(frame) = self.frames.pop() {
            self.record_frame(frame, end);
        }
        self.root = Value::Open;
    }

    fn fail(&mut self, event: &Event, message: &'static str) {
        self.structure.error = Some(ParseError {
            offset: event.start,
            line: event.line,
            column: event.column,
            message,
        });
    }

    /// Ends the pass at `eof`, which the lexer reached at `line` and `column` in `state`.
    /// Collections still open end where the well-formed part does, so the tree lists what came
    /// before the error.
    pub(crate) fn finish(
        mut self,
        eof: u64,
        line: u64,
        column: u64,
        state: LineState,
    ) -> Structure {
        if self.structure.error.is_none() && (state.quote != 0 || state.flow > 0) {
            self.structure.error = Some(ParseError {
                offset: eof,
                line,
                column,
                message: if state.quote != 0 {
                    "unterminated quoted scalar"
                } else {
                    "unclosed flow collection"
                },
            });
        }
        let end = self
            .structure
            .error
            .as_ref()
            .map_or(eof, |error| error.offset);
        self.structure.end = end;
        self.end_document(end);
        let start = self.structure.start;
        let documents = std::mem::replace(&mut self.documents, Counter::new());
        let mut structure = self.structure;
        structure.documents = documents.entry(start, end);
        structure.entries.sort_unstable_by_key(|entry| entry.start);
        structure.entries.shrink_to_fit();
        structure
    }
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
