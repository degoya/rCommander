//! A walk over the valid part of the file for the tree: one value at a time, jumping over the
//! values the structure indexed and reading only the small ones it did not.

use crate::lexer::{Kind, Lexer};
use crate::reader::{Bytes, Reader};
use crate::structure::Structure;
use crate::types::{Failure, Source};

/// The window a walk reads through.
const WALK_CHUNK: u32 = 64 * 1024;
/// The bytes of a key or scalar kept for its label or value.
const CAPTURE_BYTES: usize = 256;
/// The characters of a label or value shown; longer ones end in an ellipsis.
pub const PREVIEW_CHARS: usize = 80;

/// A value as the tree shows it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Container { object: bool, count: u64 },
    Scalar { preview: String },
}

pub(crate) struct Walker<'s, 'a, S: Source> {
    lexer: Lexer<Reader<'a, S>>,
    structure: &'s Structure,
}

impl<'s, 'a, S: Source> Walker<'s, 'a, S> {
    /// A walk starting at `offset`.
    pub(crate) fn new(src: &'a mut S, structure: &'s Structure, offset: u64) -> Self {
        Self {
            lexer: Lexer::new(Reader::new(src, offset, WALK_CHUNK)).with_capture(CAPTURE_BYTES),
            structure,
        }
    }

    /// The next byte that is not whitespace and lies in the valid part, without taking it.
    pub(crate) fn peek(&mut self) -> Result<Option<u8>, Failure> {
        let byte = self.lexer.skip_whitespace(&mut |_, _| {})?;
        if self.lexer.bytes_mut().offset() >= self.structure.end {
            return Ok(None);
        }
        Ok(byte)
    }

    /// The next lexeme in the valid part, keeping its first bytes in [`Self::captured`].
    pub(crate) fn next(&mut self) -> Result<Option<crate::lexer::Lexeme>, Failure> {
        if self.peek()?.is_none() {
            return Ok(None);
        }
        self.lexer.next()
    }

    pub(crate) fn captured(&self) -> &[u8] {
        self.lexer.captured()
    }

    /// The value at the walk's position and its start, the walk left behind it; `None` where
    /// the valid part ends first.
    pub(crate) fn value(&mut self) -> Result<Option<(u64, Shape)>, Failure> {
        let Some(first) = self.peek()? else {
            return Ok(None);
        };
        let start = self.lexer.bytes_mut().offset();
        if let Some(entry) = self.structure.entry(start) {
            let shape = if matches!(first, b'{' | b'[') {
                Shape::Container {
                    object: first == b'{',
                    count: entry.count,
                }
            } else {
                let length = (entry.end - start).min(CAPTURE_BYTES as u64) as u32;
                let head = self.lexer.bytes_mut().source().read_at(start, length)?;
                Shape::Scalar {
                    preview: preview(&head, false),
                }
            };
            self.lexer.bytes_mut().seek(entry.end);
            return Ok(Some((start, shape)));
        }
        let Some(lexeme) = self.lexer.next()? else {
            return Ok(None);
        };
        let shape = if lexeme.kind.is_open() {
            Shape::Container {
                object: lexeme.kind == Kind::BeginObject,
                count: self.count_children(lexeme.kind == Kind::BeginObject)?,
            }
        } else {
            let whole = lexeme.end - lexeme.start == self.captured().len() as u64;
            Shape::Scalar {
                preview: preview(self.captured(), whole),
            }
        };
        Ok(Some((start, shape)))
    }

    /// Counts the children of the small container just opened and leaves the walk behind it:
    /// keys in an object, values in an array.
    fn count_children(&mut self, object: bool) -> Result<u64, Failure> {
        let (mut depth, mut count, mut expect_child) = (1u32, 0u64, true);
        while let Some(lexeme) = self.next()? {
            let at_top = depth == 1;
            match lexeme.kind {
                Kind::BeginObject | Kind::BeginArray => {
                    if at_top && expect_child && !object {
                        count += 1;
                        expect_child = false;
                    }
                    depth += 1;
                }
                Kind::EndObject | Kind::EndArray => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Kind::Comma => expect_child |= at_top,
                Kind::Colon => {}
                Kind::String | Kind::Number | Kind::Literal | Kind::Invalid(_) => {
                    if at_top && expect_child && (!object || lexeme.kind == Kind::String) {
                        count += 1;
                        expect_child = false;
                    }
                }
            }
        }
        Ok(count)
    }
}

/// A scalar as the tree shows it: a string decoded in its quotes, anything else as written; cut
/// at [`PREVIEW_CHARS`]. `whole`: these are all its bytes.
pub(crate) fn preview(bytes: &[u8], whole: bool) -> String {
    if bytes.first() == Some(&b'"') {
        let inner = bytes.get(1..).unwrap_or_default();
        let inner = if whole {
            inner.strip_suffix(b"\"").unwrap_or(inner)
        } else {
            inner
        };
        let text = shorten(decode_string(inner, !whole), PREVIEW_CHARS, !whole);
        return if text.ends_with('\u{2026}') {
            format!("\"{text}")
        } else {
            format!("\"{text}\"")
        };
    }
    shorten(
        String::from_utf8_lossy(bytes).into_owned(),
        PREVIEW_CHARS,
        !whole,
    )
}

/// A key as a label: decoded, without its quotes, cut at [`PREVIEW_CHARS`].
pub(crate) fn key_label(bytes: &[u8], whole: bool) -> String {
    let inner = bytes.get(1..).unwrap_or_default();
    let inner = if whole {
        inner.strip_suffix(b"\"").unwrap_or(inner)
    } else {
        inner
    };
    shorten(decode_string(inner, !whole), PREVIEW_CHARS, !whole)
}

/// At most `chars` characters, an ellipsis when cut (or when `cut` already).
fn shorten(text: String, chars: usize, cut: bool) -> String {
    match text.char_indices().nth(chars) {
        Some((at, _)) => format!("{}\u{2026}", text.get(..at).unwrap_or_default()),
        None if cut => format!("{text}\u{2026}"),
        None => text,
    }
}

/// The text of a string's contents with its escapes resolved; an unknown escape stays as written.
/// `cut`: the bytes stop early, so a character cut in half at the end is dropped.
pub(crate) fn decode_string(bytes: &[u8], cut: bool) -> String {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while let Some(&byte) = bytes.get(index) {
        index += 1;
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let Some(&escape) = bytes.get(index) else {
            break;
        };
        index += 1;
        let simple = match escape {
            b'"' => Some(b'"'),
            b'\\' => Some(b'\\'),
            b'/' => Some(b'/'),
            b'b' => Some(0x08),
            b'f' => Some(0x0C),
            b'n' => Some(b'\n'),
            b'r' => Some(b'\r'),
            b't' => Some(b'\t'),
            _ => None,
        };
        if let Some(simple) = simple {
            out.push(simple);
            continue;
        }
        if escape != b'u' {
            out.extend_from_slice(&[b'\\', escape]);
            continue;
        }
        let Some(high) = hex4(bytes, index) else {
            break;
        };
        index += 4;
        let mut code = u32::from(high);
        if (0xD800..0xDC00).contains(&high)
            && bytes.get(index..index + 2) == Some(b"\\u".as_slice())
            && let Some(low) = hex4(bytes, index + 2)
            && (0xDC00..0xE000).contains(&low)
        {
            index += 6;
            code = 0x10000 + ((code - 0xD800) << 10) + (u32::from(low) - 0xDC00);
        }
        let character = char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER);
        let mut buffer = [0u8; 4];
        out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
    }
    let mut text = String::from_utf8_lossy(&out).into_owned();
    if cut && text.ends_with(char::REPLACEMENT_CHARACTER) {
        text.pop();
    }
    text
}

fn hex4(bytes: &[u8], at: usize) -> Option<u16> {
    let digits = bytes.get(at..at + 4)?;
    let text = std::str::from_utf8(digits).ok()?;
    u16::from_str_radix(text, 16).ok()
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
