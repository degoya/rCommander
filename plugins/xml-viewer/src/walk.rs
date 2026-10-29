//! A walk over the well-formed part of the file for the tree: one element at a time, jumping over
//! the elements the structure indexed and reading only the small ones it did not.

use crate::lexer::{Kind, LexState, Lexeme, Lexer};
use crate::reader::{Bytes, Reader, SliceBytes};
use crate::structure::Structure;
use crate::types::{Failure, Source};

/// The window a walk reads through.
const WALK_CHUNK: u32 = 64 * 1024;
/// The bytes of a name, value or text lexeme kept for a label or a value.
const CAPTURE_BYTES: usize = 256;
/// The characters of a label or value shown; longer ones end in an ellipsis.
pub const PREVIEW_CHARS: usize = 80;
/// The bytes of text gathered for a value: enough for its characters once whitespace is folded.
const TEXT_BYTES: usize = 4 * PREVIEW_CHARS;

/// An element as the tree shows it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Element {
    /// `<name attr="value" ...>`, cut.
    pub(crate) label: String,
    /// Its child elements.
    pub(crate) count: u64,
    /// The text of an element without child elements, folded and cut; `None` when it has none.
    pub(crate) text: Option<String>,
}

pub(crate) struct Walker<'s, 'a, S: Source> {
    lexer: Lexer<Reader<'a, S>>,
    structure: &'s Structure,
}

impl<'s, 'a, S: Source> Walker<'s, 'a, S> {
    /// A walk starting at `offset`, between markup.
    pub(crate) fn new(src: &'a mut S, structure: &'s Structure, offset: u64) -> Self {
        Self {
            lexer: Lexer::new(Reader::new(src, offset, WALK_CHUNK)).with_capture(CAPTURE_BYTES),
            structure,
        }
    }

    /// The next lexeme in the well-formed part, its first bytes in the lexer's capture.
    pub(crate) fn next(&mut self) -> Result<Option<Lexeme>, Failure> {
        if self.lexer.skip(&mut |_, _, _| {})?.is_none()
            || self.lexer.bytes_mut().offset() >= self.structure.end
        {
            return Ok(None);
        }
        self.lexer.next()
    }

    /// Moves to `offset`, the end of a tag or an element.
    fn seek(&mut self, offset: u64) {
        self.lexer.bytes_mut().seek(offset);
        self.lexer.set_state(LexState::default());
    }

    /// Reads the start tag whose `<` was just read: true when the element has content to read.
    pub(crate) fn skip_head(&mut self) -> Result<bool, Failure> {
        while let Some(lexeme) = self.next()? {
            match lexeme.kind {
                Kind::Close => return Ok(true),
                Kind::EmptyClose => return Ok(false),
                _ => {}
            }
        }
        Ok(false)
    }

    /// The element whose `<` is `open`, read to its end: its label, child count and text.
    pub(crate) fn element(&mut self, open: &Lexeme) -> Result<Element, Failure> {
        let structure = self.structure;
        let entry = structure.entry(open.start);
        let mut label = Label::default();
        label.push(b"<", true);
        // The start tag, as far as the label shows it: an indexed element is left at its content.
        let mut head = None;
        while let Some(lexeme) = self.next()? {
            let captured = self.lexer.captured();
            let whole = lexeme.end - lexeme.start == captured.len() as u64;
            match lexeme.kind {
                Kind::Name => label.push(captured, whole),
                Kind::AttrName => {
                    label.push(b" ", true);
                    label.push(captured, whole);
                }
                Kind::Eq | Kind::Value { .. } | Kind::Entity => label.push(captured, whole),
                Kind::Close => {
                    label.push(b">", true);
                    head = Some(true);
                    break;
                }
                Kind::EmptyClose => {
                    label.push(b"/>", true);
                    head = Some(false);
                    break;
                }
                _ => break,
            }
            if label.full()
                && let Some(entry) = entry
            {
                self.seek(entry.content);
                head = Some(entry.content < entry.end);
                break;
            }
        }
        let label = label.finish(head.is_none());
        if head != Some(true) {
            if let Some(entry) = entry {
                self.seek(entry.end);
            }
            return Ok(Element {
                label,
                count: 0,
                text: None,
            });
        }
        if let Some(entry) = entry {
            let text = if entry.count == 0 {
                self.leaf_text(entry.content, entry.end)?
            } else {
                None
            };
            self.seek(entry.end);
            return Ok(Element {
                label,
                count: entry.count,
                text,
            });
        }
        let (count, text) = self.content()?;
        Ok(Element {
            label,
            count,
            text: if count == 0 { text } else { None },
        })
    }

    /// Reads an element's content to its end tag: its child elements and its own text.
    fn content(&mut self) -> Result<(u64, Option<String>), Failure> {
        let structure = self.structure;
        let (mut depth, mut count) = (0u32, 0u64);
        let mut text = Text::default();
        while let Some(lexeme) = self.next()? {
            match lexeme.kind {
                Kind::Open => {
                    if depth == 0 {
                        count += 1;
                    }
                    if let Some(entry) = structure.entry(lexeme.start) {
                        self.seek(entry.end);
                    }
                }
                Kind::Close => depth += 1,
                Kind::EndOpen if depth == 0 => break,
                Kind::EndOpen => depth -= 1,
                Kind::Text | Kind::Entity | Kind::CData { .. } if depth == 0 => {
                    let captured = self.lexer.captured();
                    let whole = lexeme.end - lexeme.start == captured.len() as u64;
                    text.push(lexeme.kind, captured, whole, lexeme.state.spaced);
                }
                _ => {}
            }
        }
        Ok((count, text.finish()))
    }

    /// The text of an indexed element without children: from the first bytes of its content,
    /// never the whole of a large one.
    fn leaf_text(&mut self, content: u64, end: u64) -> Result<Option<String>, Failure> {
        let length = (end - content).min(TEXT_BYTES as u64) as u32;
        let bytes = self.lexer.bytes_mut().source().read_at(content, length)?;
        let bytes = bytes.get(..length as usize).unwrap_or(&bytes);
        let complete = u64::from(length) == end - content;
        let mut lexer = Lexer::new(SliceBytes::new(bytes));
        let mut text = Text::default();
        while let Ok(Some(lexeme)) = lexer.next() {
            let piece = bytes
                .get(lexeme.start as usize..lexeme.end as usize)
                .unwrap_or_default();
            let whole = complete || lexeme.end < bytes.len() as u64;
            match lexeme.kind {
                Kind::Text | Kind::Entity | Kind::CData { .. } => {
                    text.push(lexeme.kind, piece, whole, lexeme.state.spaced);
                }
                Kind::Open | Kind::EndOpen => break,
                _ => {}
            }
        }
        if !complete {
            text.cut = true;
        }
        Ok(text.finish())
    }
}

/// An element's label as it is read: at most [`PREVIEW_CHARS`] characters kept.
#[derive(Debug, Default)]
struct Label {
    text: String,
    chars: usize,
    cut: bool,
}

impl Label {
    fn push(&mut self, bytes: &[u8], whole: bool) {
        if self.full() {
            self.cut = true;
            return;
        }
        let piece = String::from_utf8_lossy(bytes);
        self.chars += piece.chars().count();
        self.text.push_str(&piece);
        self.cut |= !whole;
    }

    /// More would not be shown.
    fn full(&self) -> bool {
        self.cut || self.chars > PREVIEW_CHARS
    }

    /// `broken`: the start tag ended before its `>`.
    fn finish(self, broken: bool) -> String {
        shorten(self.text, PREVIEW_CHARS, self.cut || broken)
    }
}

/// An element's own text as it is read: at most [`TEXT_BYTES`] of it.
#[derive(Debug, Default)]
pub(crate) struct Text {
    bytes: Vec<u8>,
    cut: bool,
}

impl Text {
    /// Adds a text, entity or CDATA piece; `whole`: these are all its bytes; `spaced`: whitespace
    /// came before it.
    pub(crate) fn push(&mut self, kind: Kind, bytes: &[u8], whole: bool, spaced: bool) {
        if self.bytes.len() >= TEXT_BYTES {
            self.cut = true;
            return;
        }
        if spaced && !self.bytes.is_empty() {
            self.bytes.push(b' ');
        }
        let decoded;
        let piece: &[u8] = match kind {
            Kind::Entity if whole => {
                decoded = decode_entity(bytes);
                &decoded
            }
            Kind::CData { closed: true } if whole => bytes.strip_suffix(b"]]>").unwrap_or(bytes),
            _ => bytes,
        };
        let room = TEXT_BYTES - self.bytes.len();
        if piece.len() > room || !whole {
            self.cut = true;
        }
        self.bytes
            .extend_from_slice(piece.get(..piece.len().min(room)).unwrap_or_default());
    }

    /// The text, whitespace folded to single spaces and cut at [`PREVIEW_CHARS`]; `None` when
    /// there is none.
    pub(crate) fn finish(self) -> Option<String> {
        let mut text = String::from_utf8_lossy(&self.bytes).into_owned();
        if self.cut && text.ends_with(char::REPLACEMENT_CHARACTER) {
            text.pop();
        }
        let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if folded.is_empty() {
            return None;
        }
        Some(shorten(folded, PREVIEW_CHARS, self.cut))
    }
}

/// The text an entity reference stands for: the five XML predefines and character references;
/// any other entity as written.
pub(crate) fn decode_entity(bytes: &[u8]) -> Vec<u8> {
    let name = bytes
        .strip_prefix(b"&")
        .and_then(|rest| rest.strip_suffix(b";"))
        .unwrap_or_default();
    let character = match name {
        b"lt" => Some('<'),
        b"gt" => Some('>'),
        b"amp" => Some('&'),
        b"quot" => Some('"'),
        b"apos" => Some('\''),
        _ => name.strip_prefix(b"#").and_then(|number| {
            let (digits, radix) = match number.strip_prefix(b"x") {
                Some(hex) => (hex, 16),
                None => (number, 10),
            };
            let digits = std::str::from_utf8(digits).ok()?;
            char::from_u32(u32::from_str_radix(digits, radix).ok()?)
        }),
    };
    match character {
        Some(character) => character.to_string().into_bytes(),
        None => bytes.to_vec(),
    }
}

/// At most `chars` characters, an ellipsis when cut (or when `cut` already).
pub(crate) fn shorten(text: String, chars: usize, cut: bool) -> String {
    match text.char_indices().nth(chars) {
        Some((at, _)) => format!("{}\u{2026}", text.get(..at).unwrap_or_default()),
        None if cut => format!("{text}\u{2026}"),
        None => text,
    }
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
