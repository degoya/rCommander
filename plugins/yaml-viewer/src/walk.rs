//! A walk over the well-formed part of the file for the tree: one node at a time, jumping over
//! the collections and values the structure indexed and reading only the small ones it did not.

use crate::events::{Event, EventKind, Events};
use crate::lexer::Lexer;
use crate::reader::Reader;
use crate::structure::Structure;
use crate::types::{Failure, Source};

/// The window a walk reads through.
const WALK_CHUNK: u32 = 64 * 1024;
/// The bytes of a lexeme kept for a label or preview.
const CAPTURE_BYTES: usize = 256;

/// A node's value as the tree shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// No value: null.
    Empty,
    /// A scalar, an alias, a flow collection or a block scalar, previewed.
    Leaf { preview: String },
    /// A block collection: its first child's start and column, its kind and child count.
    Collection {
        start: u64,
        col: u32,
        seq: bool,
        count: u64,
    },
}

pub(crate) struct Walker<'s, 'a, S: Source> {
    events: Events<Reader<'a, S>>,
    structure: &'s Structure,
    back: Option<Event>,
}

impl<'s, 'a, S: Source> Walker<'s, 'a, S> {
    /// A walk from a node at `offset`, byte column `col` of its line.
    pub(crate) fn at_node(src: &'a mut S, structure: &'s Structure, offset: u64, col: u32) -> Self {
        let lexer = Lexer::mid_line(Reader::new(src, offset, WALK_CHUNK), col);
        Self::over(lexer, structure)
    }

    /// A walk from `offset`, the start of a line in plain block context.
    pub(crate) fn at_line(src: &'a mut S, structure: &'s Structure, offset: u64) -> Self {
        Self::over(Lexer::new(Reader::new(src, offset, WALK_CHUNK)), structure)
    }

    fn over(lexer: Lexer<Reader<'a, S>>, structure: &'s Structure) -> Self {
        Self {
            events: Events::new(lexer.with_capture(CAPTURE_BYTES), structure.end, true),
            structure,
            back: None,
        }
    }

    /// The next event of the well-formed part.
    pub(crate) fn next(&mut self) -> Result<Option<Event>, Failure> {
        match self.back.take() {
            Some(event) => Ok(Some(event)),
            None => self.events.next(),
        }
    }

    /// Hands `event` back, to be read next again.
    fn unread(&mut self, event: Event) {
        self.back = Some(event);
    }

    fn jump(&mut self, offset: u64) {
        self.back = None;
        self.events.jump(offset);
    }

    /// The value that `event` starts - a deeper key or item, or a value - leaving the walk
    /// behind it.
    pub(crate) fn value(&mut self, event: &Event) -> Result<Shape, Failure> {
        match event.kind {
            EventKind::Key | EventKind::Item => self.collection(event),
            EventKind::Value => {
                if let Some(end) = self.structure.entry(event.start).map(|entry| entry.end) {
                    self.jump(end);
                }
                Ok(Shape::Leaf {
                    preview: event.text.clone(),
                })
            }
            EventKind::DocStart | EventKind::DocEnd | EventKind::Invalid(_) => Ok(Shape::Empty),
        }
    }

    /// The collection whose first child `first` is: its child count, the walk left behind it.
    fn collection(&mut self, first: &Event) -> Result<Shape, Failure> {
        let seq = first.kind == EventKind::Item;
        let shape = |count| Shape::Collection {
            start: first.start,
            col: first.col,
            seq,
            count,
        };
        if let Some((end, count)) = self
            .structure
            .entry(first.start)
            .map(|entry| (entry.end, entry.count))
        {
            self.jump(end);
            return Ok(shape(count));
        }
        let mut count = 1u64;
        // The last key's value has not come yet: an item at the key's column starts it, as an
        // indentless sequence, and the items after it belong to it too.
        let (mut open, mut indentless) = (true, false);
        while let Some(event) = self.next()? {
            let ends = match event.kind {
                EventKind::DocStart | EventKind::DocEnd | EventKind::Invalid(_) => true,
                _ if event.col < first.col => true,
                _ if event.col > first.col => {
                    open = false;
                    false
                }
                EventKind::Item if seq => {
                    count += 1;
                    false
                }
                EventKind::Key if !seq => {
                    count += 1;
                    (open, indentless) = (true, false);
                    false
                }
                EventKind::Item if open || indentless => {
                    (open, indentless) = (false, true);
                    false
                }
                EventKind::Key | EventKind::Item | EventKind::Value => true,
            };
            if ends {
                self.unread(event);
                break;
            }
        }
        Ok(shape(count))
    }
}
