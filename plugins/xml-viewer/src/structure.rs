//! The structure `open` learns in its one pass: whether the XML is well-formed and where it first
//! is not, where the root element starts, and an index that lets the tree list any element's
//! children without reading what lies between.
//!
//! The index holds an [`Entry`] for every element of at least `min_size` bytes and every element
//! of more than [`PAGE`] child elements: its end, so a walk jumps over it, where its content
//! starts, its child count, and an anchor - the offset of a child's `<` - every `anchor_step`
//! children, so a page in the middle of a long list starts near its first child. Elements below
//! `min_size` are read when a walk passes them, which bounds that read. The index never takes more
//! than [`MAX_INDEX_BYTES`]: past that `min_size` doubles and the smaller entries go, so a huge
//! file costs walks, not memory - the JSON viewer's index, for elements.
//!
//! Well-formed is what one pass can tell without a DTD: tags nested and closed by the same name,
//! one root element, attributes as `name="value"`, comments, CDATA sections, processing
//! instructions and the DOCTYPE closed, entity references well-formed. Not checked: duplicate
//! attributes, undeclared entities, namespace prefixes, character ranges.

use crate::lexer::{Kind, Lexeme, Mode};

/// Child elements per page of the tree: an element with more is shown as ranges.
pub const PAGE: u64 = 1000;
/// The most memory the index takes.
pub const MAX_INDEX_BYTES: usize = 8 << 20;
/// The deepest nesting accepted.
pub const MAX_DEPTH: usize = 512;
/// The bytes of an element's name kept to match its end tag; a longer name is matched by these
/// and its length.
pub const NAME_BYTES: usize = 256;

const FIRST_MIN_SIZE: u64 = 4096;
/// The most anchors one element keeps; past that every second goes and the step doubles.
const MAX_ANCHORS: usize = 4096;
/// The characters of an element's name an error message shows.
const NAME_CHARS: usize = 64;

/// Where the XML first stops being well-formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Where the well-formed part ends: the lexeme in error, or the end of the file for an
    /// element left open.
    pub offset: u64,
    /// 1-based: where to look - the lexeme in error, or the start tag left open.
    pub line: u64,
    /// 1-based, in characters.
    pub column: u64,
    pub message: String,
}

/// One indexed element.
#[derive(Debug)]
pub(crate) struct Entry {
    /// Its `<`.
    pub(crate) start: u64,
    pub(crate) end: u64,
    /// After its start tag's `>`; its end for an empty element.
    pub(crate) content: u64,
    /// Its child elements.
    pub(crate) count: u64,
    pub(crate) anchor_step: u64,
    /// The offset of child `(i + 1) * anchor_step`.
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
    /// Where the well-formed part ends.
    pub(crate) end: u64,
}

impl Structure {
    /// The entry of the element starting at `start`.
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
    /// Before the root element.
    Prolog,
    /// Inside an element.
    Content,
    /// After the root element.
    Epilog,
    /// After `<` or `</` (`end`).
    Name {
        end: bool,
    },
    /// In a start tag: an attribute, `>` or `/>`.
    Attr,
    Eq,
    Value,
    /// Inside a value whose closing quote is still to come.
    InValue,
    /// In an end tag after its name.
    EndClose,
}

impl Expect {
    fn message(self) -> &'static str {
        match self {
            Self::Prolog => "expected the root element",
            Self::Content => "unexpected markup",
            Self::Epilog => "unexpected content after the root element",
            Self::Name { .. } => "expected a name",
            Self::Attr => "expected an attribute, '>' or '/>'",
            Self::Eq => "expected '=' after the attribute name",
            Self::Value => "expected a quoted attribute value",
            Self::InValue => "unterminated attribute value",
            Self::EndClose => "expected '>' after the end tag's name",
        }
    }
}

/// A place in the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Place {
    offset: u64,
    line: u64,
    column: u64,
}

impl Place {
    fn of(lexeme: &Lexeme) -> Self {
        Self {
            offset: lexeme.start,
            line: lexeme.line,
            column: lexeme.column,
        }
    }
}

/// An element whose end tag is still to come.
#[derive(Debug)]
struct Frame {
    place: Place,
    name: Vec<u8>,
    name_len: u64,
    content: u64,
    count: u64,
    anchor_step: u64,
    anchors: Vec<u64>,
}

impl Frame {
    /// A child element starts at `offset`.
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

/// The checker of `open`'s pass, fed one lexeme at a time.
pub(crate) struct Builder {
    stack: Vec<Frame>,
    expect: Expect,
    structure: Structure,
    /// The `<` or `</` of the tag being read.
    tag: Place,
    /// The name of the start tag being read: its first bytes and its length.
    name: Vec<u8>,
    name_len: u64,
    /// Where the last construct that may span lines started: a comment, PI, DOCTYPE, CDATA
    /// section or attribute value.
    construct: Place,
}

impl Builder {
    pub(crate) fn new() -> Self {
        Self {
            stack: Vec::new(),
            expect: Expect::Prolog,
            structure: Structure {
                min_size: FIRST_MIN_SIZE,
                ..Structure::default()
            },
            tag: Place::default(),
            name: Vec::new(),
            name_len: 0,
            construct: Place::default(),
        }
    }

    /// Takes `lexeme`; `captured` holds its first bytes (at least [`NAME_BYTES`] of them).
    pub(crate) fn feed(&mut self, lexeme: &Lexeme, captured: &[u8]) {
        if self.structure.error.is_some() {
            return;
        }
        let place = Place::of(lexeme);
        let kind = lexeme.kind;
        if !lexeme.state.mode.is_construct() && starts_construct(kind) {
            self.construct = place;
        }
        if let Kind::Invalid(problem) = kind {
            return self.fail(place, problem.message().to_owned());
        }
        let length = lexeme.end - lexeme.start;
        match (self.expect, kind) {
            (
                Expect::Prolog | Expect::Content | Expect::Epilog,
                Kind::Comment { .. } | Kind::Pi { .. },
            )
            | (Expect::Prolog, Kind::Doctype { .. })
            | (Expect::Content, Kind::Text | Kind::Entity | Kind::CDataOpen | Kind::CData { .. })
            | (Expect::InValue, Kind::Entity) => {}
            (Expect::Content | Expect::Epilog, Kind::Doctype { .. }) => {
                self.fail(place, "a DOCTYPE after the root element began".to_owned());
            }
            (
                Expect::Prolog | Expect::Epilog,
                Kind::Text | Kind::Entity | Kind::CDataOpen | Kind::CData { .. },
            ) => self.fail(place, "text outside the root element".to_owned()),
            (Expect::Prolog | Expect::Content, Kind::Open) => {
                self.tag = place;
                self.expect = Expect::Name { end: false };
            }
            (Expect::Epilog, Kind::Open) => {
                self.fail(place, "a second root element".to_owned());
            }
            (Expect::Content, Kind::EndOpen) => {
                self.tag = place;
                self.expect = Expect::Name { end: true };
            }
            (Expect::Prolog | Expect::Epilog, Kind::EndOpen) => {
                self.fail(place, "an end tag without its start tag".to_owned());
            }
            (Expect::Name { end: false }, Kind::Name) => self.start_element(captured, length),
            (Expect::Name { end: true }, Kind::Name) => self.end_name(captured, length),
            (Expect::Attr, Kind::AttrName) => self.expect = Expect::Eq,
            (Expect::Eq, Kind::Eq) => self.expect = Expect::Value,
            (Expect::Value | Expect::InValue, Kind::Value { closed }) => {
                self.expect = if closed {
                    Expect::Attr
                } else {
                    Expect::InValue
                };
            }
            (Expect::Attr, Kind::Close) => self.open_element(lexeme.end),
            (Expect::Attr, Kind::EmptyClose) => {
                self.structure.record(Entry {
                    start: self.tag.offset,
                    end: lexeme.end,
                    content: lexeme.end,
                    count: 0,
                    anchor_step: PAGE,
                    anchors: Box::default(),
                });
                self.after_element();
            }
            (Expect::EndClose, Kind::EndClose) => {
                if let Some(frame) = self.stack.pop() {
                    self.record_frame(frame, lexeme.end);
                }
                self.after_element();
            }
            (expect, _) => self.fail(place, expect.message().to_owned()),
        }
    }

    fn start_element(&mut self, captured: &[u8], length: u64) {
        match self.stack.last_mut() {
            Some(frame) => frame.child(self.tag.offset),
            None => self.structure.root = Some(self.tag.offset),
        }
        self.name.clear();
        self.name
            .extend_from_slice(captured.get(..NAME_BYTES).unwrap_or(captured));
        self.name_len = length;
        self.expect = Expect::Attr;
    }

    fn open_element(&mut self, content: u64) {
        if self.stack.len() >= MAX_DEPTH {
            return self.fail(self.tag, "nesting too deep".to_owned());
        }
        self.stack.push(Frame {
            place: self.tag,
            name: std::mem::take(&mut self.name),
            name_len: self.name_len,
            content,
            count: 0,
            anchor_step: PAGE,
            anchors: Vec::new(),
        });
        self.expect = Expect::Content;
    }

    /// The name of an end tag: the open element's, or the error.
    fn end_name(&mut self, captured: &[u8], length: u64) {
        let captured = captured.get(..NAME_BYTES).unwrap_or(captured);
        let message = match self.stack.last() {
            Some(frame) if frame.name_len == length && frame.name == captured => {
                self.expect = Expect::EndClose;
                return;
            }
            Some(frame) => format!(
                "expected </{}>, found </{}>",
                shown(&frame.name, frame.name_len),
                shown(captured, length)
            ),
            None => "an end tag without its start tag".to_owned(),
        };
        self.fail(self.tag, message);
    }

    fn record_frame(&mut self, frame: Frame, end: u64) {
        self.structure.record(Entry {
            start: frame.place.offset,
            end,
            content: frame.content,
            count: frame.count,
            anchor_step: frame.anchor_step,
            anchors: frame.anchors.into_boxed_slice(),
        });
    }

    fn after_element(&mut self) {
        self.expect = if self.stack.is_empty() {
            Expect::Epilog
        } else {
            Expect::Content
        };
    }

    fn fail(&mut self, place: Place, message: String) {
        self.structure.error = Some(ParseError {
            offset: place.offset,
            line: place.line,
            column: place.column,
            message,
        });
    }

    /// Ends the pass at `eof`, which the lexer reached at `line` and `column` in `mode`. Elements
    /// still open end where the well-formed part does, so the tree lists what came before the
    /// error.
    pub(crate) fn finish(mut self, eof: u64, line: u64, column: u64, mode: Mode) -> Structure {
        if self.structure.error.is_none() {
            self.check_end(
                Place {
                    offset: eof,
                    line,
                    column,
                },
                mode,
            );
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

    /// What the end of the file leaves open: a construct, a tag, an element, or no root at all.
    fn check_end(&mut self, eof: Place, mode: Mode) {
        let construct = match mode {
            Mode::Comment => Some("unterminated comment"),
            Mode::Pi => Some("unterminated processing instruction"),
            Mode::Doctype { .. } => Some("unterminated DOCTYPE"),
            Mode::CData => Some("unterminated CDATA section"),
            Mode::Value(_) => Some("unterminated attribute value"),
            _ if self.expect == Expect::InValue => Some("unterminated attribute value"),
            _ => None,
        };
        if let Some(message) = construct {
            return self.fail(self.construct, message.to_owned());
        }
        if matches!(
            self.expect,
            Expect::Name { .. } | Expect::Attr | Expect::Eq | Expect::Value | Expect::EndClose
        ) {
            return self.fail(self.tag, "unterminated tag".to_owned());
        }
        if let Some(frame) = self.stack.last() {
            let message = format!("<{}> is not closed", shown(&frame.name, frame.name_len));
            let place = frame.place;
            self.structure.error = Some(ParseError {
                offset: eof.offset,
                line: place.line,
                column: place.column,
                message,
            });
        } else if self.structure.root.is_none() {
            self.fail(eof, "no root element".to_owned());
        }
    }
}

/// A lexeme that may start a construct spanning lines.
fn starts_construct(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Comment { .. }
            | Kind::Pi { .. }
            | Kind::Doctype { .. }
            | Kind::CDataOpen
            | Kind::Value { .. }
    )
}

/// A name for a message: at most [`NAME_CHARS`] characters, an ellipsis when cut.
fn shown(name: &[u8], length: u64) -> String {
    let text = String::from_utf8_lossy(name);
    let mut shown: String = text.chars().take(NAME_CHARS).collect();
    if shown.len() < text.len() || (name.len() as u64) < length {
        shown.push('\u{2026}');
    }
    shown
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
