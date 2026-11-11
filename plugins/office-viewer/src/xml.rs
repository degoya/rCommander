//! One XML part read as a stream of events, quick-xml underneath: elements by their local name
//! (Office writes the same names under `w:`, `a:`, `p:` or none), text with the five predefined
//! entities and character references resolved, and nothing else expanded - a DOCTYPE's own
//! entities stay unexpanded, so no entity bomb can grow. Nesting deeper than the limit fails
//! with `too_deep`; the parser keeps no stack of its own, so that is the only depth there is.
//! A part is read as UTF-8 (a UTF-8 BOM is skipped): since quick-xml 0.42 every event is checked,
//! so a byte that is not UTF-8 anywhere in the part - a name, an attribute, a comment - makes it
//! damaged, as a part in UTF-16 always was.

use std::borrow::Cow;
use std::io::{BufReader, Read};

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::limits::code;
use crate::source_io::{damaged, failure_of_io};
use crate::types::{Failure, FailureKind};

/// What [`Xml::next`] read.
pub(crate) enum Ev<'a> {
    Start(BytesStart<'a>),
    Empty(BytesStart<'a>),
    End(BytesEnd<'a>),
    /// Text, a CDATA section or a resolved reference.
    Text(Cow<'a, str>),
    /// A comment, a declaration, a processing instruction, a DOCTYPE or an unknown entity.
    Other,
    Eof,
}

/// One XML part.
pub(crate) struct Xml<R: Read> {
    reader: Reader<BufReader<R>>,
    buf: Vec<u8>,
    depth: usize,
    max_depth: usize,
}

impl<R: Read> Xml<R> {
    pub(crate) fn new(inner: R, max_depth: usize) -> Self {
        Self {
            reader: Reader::from_reader(BufReader::new(inner)),
            buf: Vec::new(),
            depth: 0,
            max_depth,
        }
    }

    /// The next event.
    pub(crate) fn next(&mut self) -> Result<Ev<'_>, Failure> {
        self.buf.clear();
        let event = self
            .reader
            .read_event_into(&mut self.buf)
            .map_err(failure_of_xml)?;
        Ok(match event {
            Event::Start(start) => {
                self.depth += 1;
                if self.depth > self.max_depth {
                    return Err(Failure::coded(FailureKind::BadData, code::TOO_DEEP));
                }
                Ev::Start(start)
            }
            Event::Empty(start) => Ev::Empty(start),
            Event::End(end) => {
                self.depth = self.depth.saturating_sub(1);
                Ev::End(end)
            }
            Event::Text(text) => Ev::Text(text.xml10_content()),
            Event::CData(data) => Ev::Text(data.into_inner()),
            Event::GeneralRef(reference) => {
                if reference.is_char_ref() {
                    match reference.resolve_char_ref() {
                        Ok(Some(character)) => Ev::Text(Cow::Owned(character.to_string())),
                        _ => Ev::Other,
                    }
                } else {
                    match resolve_predefined_entity(&reference) {
                        Some(text) => Ev::Text(Cow::Borrowed(text)),
                        None => Ev::Other,
                    }
                }
            }
            Event::Eof => Ev::Eof,
            _ => Ev::Other,
        })
    }
}

/// The local name of a start or empty tag.
pub(crate) fn local<'x>(start: &'x BytesStart<'_>) -> &'x [u8] {
    start.local_name().into_inner().as_bytes()
}

/// The local name of an end tag.
pub(crate) fn local_end<'x>(end: &'x BytesEnd<'_>) -> &'x [u8] {
    end.local_name().into_inner().as_bytes()
}

/// The value of the attribute whose local name is `name` and that has no prefix.
pub(crate) fn attr(start: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    find_attr(start, name, false)
}

/// The value of the prefixed attribute whose local name is `name`: `r:id`, which `sldId` writes
/// beside its own `id`.
pub(crate) fn prefixed_attr(start: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    find_attr(start, name, true)
}

fn find_attr(start: &BytesStart<'_>, name: &[u8], prefixed: bool) -> Option<String> {
    let mut attributes = start.attributes();
    attributes.with_checks(false);
    attributes
        .flatten()
        .find(|attribute| {
            attribute.key.prefix().is_some() == prefixed
                && attribute.key.local_name().into_inner().as_bytes() == name
        })
        .and_then(|attribute| {
            attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(Cow::into_owned)
        })
}

/// A failure of the XML reader: the host's or a limit's when one travels inside, else damaged.
fn failure_of_xml(error: quick_xml::Error) -> Failure {
    match error {
        quick_xml::Error::Io(error) => failure_of_io(&error),
        _ => damaged(),
    }
}

#[cfg(test)]
#[path = "xml_tests.rs"]
mod tests;
