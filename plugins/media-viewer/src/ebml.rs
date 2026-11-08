//! EBML, the binary format of Matroska and WebM: variable-length integers, element IDs and
//! elements, read through `read-at` without holding a body bigger than a value.
//!
//! A variable-length integer is at most eight bytes (a first byte of zero is bad data); the value
//! of all ones is an unknown size. An element ID is at most four bytes and never all ones. Every
//! element's end is checked against its parent's with `checked_add`; one that does not fit is
//! `bad-data` ([`codes::BAD_VIDEO`]).

use crate::codes;
use crate::io::{SourceReader, be_uint};
use crate::types::{Failure, Source};

/// The most elements visited in one file.
pub const MAX_ELEMENTS: u32 = 100_000;
/// The most children listed of one element.
const MAX_CHILDREN: usize = 4096;
/// The most bytes of a string read.
const MAX_TEXT: usize = 4096;

fn bad() -> Failure {
    Failure::bad(codes::BAD_VIDEO)
}

/// One element: its ID and where its body is; `end` is `None` for an unknown size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Element {
    pub id: u32,
    pub body: u64,
    pub end: Option<u64>,
}

impl Element {
    /// The end of an element whose size must be known.
    pub fn known_end(&self) -> Result<u64, Failure> {
        self.end.ok_or_else(bad)
    }
}

/// The variable-length integer at the start of `bytes`: its length, and its value without the
/// length marker - `None` when every value bit is one (an unknown size).
pub fn vint(bytes: &[u8]) -> Result<(usize, Option<u64>), Failure> {
    let first = *bytes.first().ok_or_else(bad)?;
    if first == 0 {
        return Err(bad());
    }
    let len = first.leading_zeros() as usize + 1;
    let raw = bytes.get(..len).ok_or_else(bad)?;
    let mut value = u64::from(first) & ((1u64 << (8 - len)) - 1);
    for byte in raw.iter().skip(1) {
        value = (value << 8) | u64::from(*byte);
    }
    let all_ones = (1u64 << (7 * len)) - 1;
    Ok((len, (value != all_ones).then_some(value)))
}

/// The element ID at the start of `bytes`, marker included: its length and the ID. Longer than
/// four bytes, or all ones, is bad data.
pub fn element_id(bytes: &[u8]) -> Result<(usize, u32), Failure> {
    let (len, value) = vint(bytes)?;
    if len > 4 || value.is_none() {
        return Err(bad());
    }
    let raw = bytes.get(..len).ok_or_else(bad)?;
    let id = u32::try_from(be_uint(raw)).map_err(|_| bad())?;
    Ok((len, id))
}

/// A walker of elements over a [`SourceReader`], counting the elements it visits.
pub struct Elements<'r, 'a, S: Source> {
    reader: &'r mut SourceReader<'a, S>,
    visited: u32,
}

impl<'r, 'a, S: Source> Elements<'r, 'a, S> {
    pub fn new(reader: &'r mut SourceReader<'a, S>) -> Self {
        Self { reader, visited: 0 }
    }

    /// The element at `at` inside a parent that ends at `end`; `None` at the parent's end.
    pub fn header(&mut self, at: u64, end: u64) -> Result<Option<Element>, Failure> {
        if at >= end {
            return Ok(None);
        }
        self.visited += 1;
        if self.visited > MAX_ELEMENTS {
            return Err(bad());
        }
        let head = self.reader.head(at, 12)?;
        let (id_len, id) = element_id(&head)?;
        let (size_len, size) = vint(head.get(id_len..).unwrap_or_default())?;
        let body = at + (id_len + size_len) as u64;
        if body > end {
            return Err(bad());
        }
        let element_end = match size {
            Some(size) => {
                let element_end = body.checked_add(size).ok_or_else(bad)?;
                if element_end > end {
                    return Err(bad());
                }
                Some(element_end)
            }
            None => None,
        };
        Ok(Some(Element {
            id,
            body,
            end: element_end,
        }))
    }

    /// The children of `parent`, whose size must be known, as must each child's.
    pub fn children(&mut self, parent: &Element) -> Result<Vec<Element>, Failure> {
        let end = parent.known_end()?;
        let mut children = Vec::new();
        let mut at = parent.body;
        while children.len() < MAX_CHILDREN {
            let Some(child) = self.header(at, end)? else {
                break;
            };
            at = child.known_end()?;
            children.push(child);
        }
        Ok(children)
    }

    /// The body of `element`, at most `cap` bytes of it.
    fn body(&mut self, element: &Element, cap: usize) -> Result<Vec<u8>, Failure> {
        let end = element.known_end()?;
        let length = usize::try_from(end - element.body).unwrap_or(usize::MAX);
        let bytes = self.reader.head(element.body, length.min(cap))?;
        if bytes.len() < length.min(cap) {
            return Err(bad());
        }
        Ok(bytes)
    }

    /// An unsigned integer: at most eight bytes, big-endian; none is zero.
    pub fn uint(&mut self, element: &Element) -> Result<u64, Failure> {
        if element.known_end()? - element.body > 8 {
            return Err(bad());
        }
        Ok(be_uint(&self.body(element, 8)?))
    }

    /// A float: four or eight bytes, big-endian; none is zero.
    pub fn float(&mut self, element: &Element) -> Result<f64, Failure> {
        let bytes = self.body(element, 9)?;
        match bytes.len() {
            0 => Ok(0.0),
            4 => {
                let bits = u32::try_from(be_uint(&bytes)).map_err(|_| bad())?;
                Ok(f64::from(f32::from_bits(bits)))
            }
            8 => Ok(f64::from_bits(be_uint(&bytes))),
            _ => Err(bad()),
        }
    }

    /// A string, at most [`MAX_TEXT`] bytes of it, without the zero bytes that may pad it.
    pub fn string(&mut self, element: &Element) -> Result<String, Failure> {
        let bytes = self.body(element, MAX_TEXT)?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(text.trim_end_matches('\0').to_owned())
    }
}

#[cfg(test)]
#[path = "ebml_tests.rs"]
mod tests;
