//! The DOCTYPE (RC-080-69): an internal subset declares entities, and an entity expands to
//! markup the depth scan of the bytes never sees, or to text far larger than the file - a few
//! kilobytes of references fill the store's memory, which in the component is a trap. roxmltree
//! parses without DTDs here ([`crate::budget::parsing`]), and usvg reads an SVG inside a
//! `data:` URL with them, so the declaration is blanked before either reads a document: a
//! picture with the usual `<!DOCTYPE svg PUBLIC ...>` still draws, one that uses an entity it
//! declared is no SVG any more (`not_svg`), never a trap.

use std::borrow::Cow;
use std::sync::Arc;

use crate::depth::{after, declaration_end};

const BOM: &str = "\u{feff}";

/// `text` with its DOCTYPE declaration, internal subset and all, turned into as many spaces;
/// `text` itself when its prolog has none. The prolog is read as roxmltree reads it
/// (`tokenizer::parse`): the XML declaration, spaces, comments and processing instructions, then
/// the DOCTYPE. A declaration this misses is refused by the parser, which allows none.
#[must_use]
pub fn without_doctype(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut at = if text.starts_with(BOM) { BOM.len() } else { 0 };
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let rest = bytes.get(at..).unwrap_or_default();
        if rest.starts_with(b"<?") {
            at = after(bytes, at + 2, b"?>");
        } else if rest.starts_with(b"<!--") {
            at = after(bytes, at + 4, b"-->");
        } else if rest.starts_with(b"<!DOCTYPE") {
            // `declaration_end` stops after an ASCII `>` or at the end: a character boundary.
            let end = declaration_end(bytes, at + 2);
            let mut blanked = String::with_capacity(text.len());
            blanked.push_str(&text[..at]);
            blanked.extend(std::iter::repeat_n(' ', end - at));
            blanked.push_str(&text[end..]);
            return Cow::Owned(blanked);
        } else {
            return Cow::Borrowed(text);
        }
    }
}

/// [`without_doctype`] for the bytes of a `data:` URL usvg may read as SVG; bytes that are no
/// UTF-8 stay as they are (usvg reads no SVG from them).
#[must_use]
pub fn data_without_doctype(data: Arc<Vec<u8>>) -> Arc<Vec<u8>> {
    let Ok(text) = std::str::from_utf8(&data) else {
        return data;
    };
    match without_doctype(text) {
        Cow::Borrowed(_) => data,
        Cow::Owned(blanked) => Arc::new(blanked.into_bytes()),
    }
}

#[cfg(test)]
#[path = "doctype_tests.rs"]
mod tests;
