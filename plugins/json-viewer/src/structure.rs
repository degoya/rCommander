//! The structure `open` learns in its one pass: whether the JSON is valid and where it first is
//! not, where the root value starts, and an index that lets the tree list any container's
//! children without reading what lies between.
//!
//! The index holds an [`Entry`] for every value of at least `min_size` bytes and every container
//! of more than [`PAGE`] children: its end, so a walk jumps over it, its child count, and an
//! anchor - the offset of a child - every `anchor_step` children, so a page in the middle of a
//! long list starts near its first child. Values below `min_size` are read when a walk passes
//! them, which bounds that read. The index never takes more than [`MAX_INDEX_BYTES`]: past that
//! `min_size` doubles and the smaller entries go, so a huge file costs walks, not memory.

use crate::lexer::{Kind, Lexeme};

/// Children per page of the tree: a container with more is shown as ranges.
pub const PAGE: u64 = 1000;
/// The most memory the index takes.
pub const MAX_INDEX_BYTES: usize = 8 << 20;
/// The deepest nesting accepted, as serde_json's recursion limit guards it.
pub const MAX_DEPTH: usize = 512;

const FIRST_MIN_SIZE: u64 = 4096;
/// The most anchors one container keeps; past that every second goes and the step doubles.
const MAX_ANCHORS: usize = 4096;

/// Where the JSON first stops being valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub offset: u64,
    /// 1-based.
    pub line: u64,
    /// 1-based, in characters.
    pub column: u64,
    pub message: &'static str,
}

/// One indexed value.
#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) start: u64,
    pub(crate) end: u64,
    /// Children of a container; 0 for a scalar.
    pub(crate) count: u64,
    pub(crate) anchor_step: u64,
    /// The offset of child `(i + 1) * anchor_step` (its key's, in an object).
    pub(crate) anchors: Box<[u64]>,
}

#[derive(Debug, Default)]
pub(crate) struct Structure {
    /// Sorted by `start` once the pass is done.
    entries: Vec<Entry>,
    entry_bytes: usize,
    min_size: u64,
    pub(crate) root: Option<u64>,
    pub(crate) error: Option<ParseError>,
    /// Where the valid part ends: the error's offset, or the end of the file.
    pub(crate) end: u64,
}

impl Structure {
    /// The entry of the value starting at `start`.
    pub(crate) fn entry(&self, start: u64) -> Option<&Entry> {
        self.entries
            .binary_search_by_key(&start, |entry| entry.start)
            .ok()
            .and_then(|index| self.entries.get(index))
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<Entry>() + self.entry_bytes
    }

    fn record(&mut self, entry: Entry) {
        if entry.end - entry.start < self.min_size && entry.count <= PAGE {
            return;
        }
        self.entry_bytes += entry.anchors.len() * size_of::<u64>();
        self.entries.push(entry);
        while self.worst_bytes() > MAX_INDEX_BYTES && !self.entries.is_empty() {
            self.min_size *= 2;
            let min_size = self.min_size;
            self.entries
                .retain(|entry| entry.end - entry.start >= min_size);
            self.entry_bytes = self
                .entries
                .iter()
                .map(|entry| entry.anchors.len() * size_of::<u64>())
                .sum();
        }
    }

    /// The memory the entries could take once the vector grows again: it doubles when full, so
    /// twice what is in it.
    fn worst_bytes(&self) -> usize {
        2 * self.entries.len() * size_of::<Entry>() + self.entry_bytes
    }
}

/// What the next lexeme may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Value,
    ValueOrClose,
    KeyOrClose,
    Key,
    Colon,
    CommaOrClose,
    End,
}

impl Expect {
    fn message(self) -> &'static str {
        match self {
            Self::Value | Self::ValueOrClose => "expected a value",
            Self::KeyOrClose | Self::Key => "expected a key in double quotes",
            Self::Colon => "expected ':'",
            Self::CommaOrClose => "expected ',' or a closing bracket",
            Self::End => "unexpected content after the value",
        }
    }
}

#[derive(Debug)]
struct Frame {
    start: u64,
    object: bool,
    count: u64,
    anchor_step: u64,
    anchors: Vec<u64>,
}

impl Frame {
    /// A child starts at `offset`.
    fn child(&mut self, offset: u64) {
        let index = self.count;
        self.count += 1;
        if index == 0 || !index.is_multiple_of(self.anchor_step) {
            return;
        }
        self.anchors.push(offset);
        if self.anchors.len() >= MAX_ANCHORS {
            // Keep the anchors of the doubled step: every second one.
            let mut position = 0usize;
            self.anchors.retain(|_| {
                position += 1;
                position.is_multiple_of(2)
            });
            self.anchor_step *= 2;
        }
    }
}

/// The validator of `open`'s pass, fed one lexeme at a time.
pub(crate) struct Builder {
    stack: Vec<Frame>,
    expect: Expect,
    structure: Structure,
}

impl Builder {
    pub(crate) fn new() -> Self {
        Self {
            stack: Vec::new(),
            expect: Expect::Value,
            structure: Structure {
                min_size: FIRST_MIN_SIZE,
                ..Structure::default()
            },
        }
    }

    pub(crate) fn feed(&mut self, lexeme: &Lexeme) {
        if self.structure.error.is_some() {
            return;
        }
        let kind = lexeme.kind;
        if let Kind::Invalid(problem) = kind {
            return self.fail(lexeme, problem.message());
        }
        match (self.expect, kind) {
            (Expect::Value | Expect::ValueOrClose, _) if is_value(kind) => {
                self.value(lexeme);
            }
            (Expect::ValueOrClose, Kind::EndArray) | (Expect::KeyOrClose, Kind::EndObject) => {
                self.close(lexeme);
            }
            (Expect::KeyOrClose | Expect::Key, Kind::String) => {
                if let Some(frame) = self.stack.last_mut() {
                    frame.child(lexeme.start);
                }
                self.expect = Expect::Colon;
            }
            (Expect::Colon, Kind::Colon) => self.expect = Expect::Value,
            (Expect::CommaOrClose, Kind::Comma) => {
                self.expect = match self.stack.last() {
                    Some(frame) if frame.object => Expect::Key,
                    _ => Expect::Value,
                };
            }
            (Expect::CommaOrClose, Kind::EndObject | Kind::EndArray)
                if self
                    .stack
                    .last()
                    .is_some_and(|frame| frame.object == (kind == Kind::EndObject)) =>
            {
                self.close(lexeme);
            }
            (expect, _) => self.fail(lexeme, expect.message()),
        }
    }

    fn value(&mut self, lexeme: &Lexeme) {
        match self.stack.last_mut() {
            Some(frame) if !frame.object => frame.child(lexeme.start),
            Some(_) => {}
            None => self.structure.root = Some(lexeme.start),
        }
        if lexeme.kind.is_open() {
            if self.stack.len() >= MAX_DEPTH {
                return self.fail(lexeme, "nesting too deep");
            }
            let object = lexeme.kind == Kind::BeginObject;
            self.stack.push(Frame {
                start: lexeme.start,
                object,
                count: 0,
                anchor_step: PAGE,
                anchors: Vec::new(),
            });
            self.expect = if object {
                Expect::KeyOrClose
            } else {
                Expect::ValueOrClose
            };
            return;
        }
        self.structure.record(Entry {
            start: lexeme.start,
            end: lexeme.end,
            count: 0,
            anchor_step: PAGE,
            anchors: Box::default(),
        });
        self.after_value();
    }

    fn close(&mut self, lexeme: &Lexeme) {
        if let Some(frame) = self.stack.pop() {
            self.record_frame(frame, lexeme.end);
        }
        self.after_value();
    }

    fn record_frame(&mut self, frame: Frame, end: u64) {
        self.structure.record(Entry {
            start: frame.start,
            end,
            count: frame.count,
            anchor_step: frame.anchor_step,
            anchors: frame.anchors.into_boxed_slice(),
        });
    }

    fn after_value(&mut self) {
        self.expect = if self.stack.is_empty() {
            Expect::End
        } else {
            Expect::CommaOrClose
        };
    }

    fn fail(&mut self, lexeme: &Lexeme, message: &'static str) {
        self.structure.error = Some(ParseError {
            offset: lexeme.start,
            line: lexeme.line,
            column: lexeme.column,
            message,
        });
    }

    /// Ends the pass at `eof`, which the lexer reached at `line` and `column`. Containers still
    /// open end where the valid part does, so the tree lists what came before the error.
    pub(crate) fn finish(mut self, eof: u64, line: u64, column: u64) -> Structure {
        if self.structure.error.is_none() && self.expect != Expect::End {
            self.structure.error = Some(ParseError {
                offset: eof,
                line,
                column,
                message: if self.structure.root.is_none() {
                    "no JSON value"
                } else {
                    "unexpected end of file"
                },
            });
        }
        let end = self
            .structure
            .error
            .as_ref()
            .map_or(eof, |error| error.offset);
        self.structure.end = end;
        while let Some(frame) = self.stack.pop() {
            self.record_frame(frame, end);
        }
        let mut structure = self.structure;
        structure.entries.sort_unstable_by_key(|entry| entry.start);
        structure.entries.shrink_to_fit();
        structure
    }
}

/// A lexeme that starts a value.
fn is_value(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::BeginObject | Kind::BeginArray | Kind::String | Kind::Number | Kind::Literal
    )
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
