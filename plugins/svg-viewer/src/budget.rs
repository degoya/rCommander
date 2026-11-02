//! The `<use>` budget: a few kilobytes of `<use>` elements, each repeating a group of the ones
//! before it ten times, expand into billions of elements - the SVG form of the billion laughs.
//! usvg builds every copy, so the document is counted first, in one pass over its elements, as
//! if each `<use>` were replaced by what it references, and refused beyond the budget. An SVG
//! inside a `data:` URL, which usvg reads while it builds the picture, takes its share from what
//! the document left ([`Remaining`]), once each time usvg reads it (RC-100-76).
//!
//! The count follows usvg 0.49's own rules (`usvg-0.49.0/src/parser/svgtree/parse.rs`): the ids
//! as it maps them, the `href` it follows, the reference as `svgtypes` reads it; and since
//! RC-100-77 the markers and patterns it instantiates (`expand.rs`). What resvg then draws
//! of the patterns has a budget of its own, in pixels (`pixels.rs`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use roxmltree::{Document, ParsingOptions};

use crate::expand;

/// The most elements a document may have with its `<use>` references expanded: far beyond a
/// diagram or a map, far below what fills the store's memory.
pub const MAX_EXPANDED_ELEMENTS: u64 = 1 << 20;

/// What `svgtypes` 0.16 skips as spaces (`Stream::skip_spaces`, `src/stream.rs:49-51`).
const SPACES: [char; 4] = [' ', '\t', '\n', '\r'];

/// How a document is parsed, here and by usvg (`Tree::from_str`, `src/parser/mod.rs:159-164`):
/// with DTDs, whose entities roxmltree expands under its own limits and never fetches.
#[must_use]
pub fn parsing<'input>() -> ParsingOptions<'input> {
    ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    }
}

/// The part of the budget the document left for the SVGs usvg reads from `data:` URLs while it
/// builds the picture. Once a share does not fit, it stays spent: every later one is refused, and
/// the picture is refused as too complex.
#[derive(Debug)]
pub struct Remaining {
    left: AtomicU64,
    exceeded: AtomicBool,
}

impl Remaining {
    #[must_use]
    pub fn new(left: u64) -> Self {
        Self {
            left: AtomicU64::new(left),
            exceeded: AtomicBool::new(false),
        }
    }

    /// Whether a share did not fit.
    #[must_use]
    pub fn exceeded(&self) -> bool {
        self.exceeded.load(Ordering::Relaxed)
    }

    /// Whether usvg may read `data` as the SVG it would make of it, its elements taken from what
    /// is left. Bytes that are no UTF-8 or no XML take nothing: usvg fails on them as well
    /// (`Tree::from_data`, `src/parser/mod.rs:105-120`).
    pub fn take_svg(&self, data: &[u8]) -> bool {
        if self.exceeded() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            return true;
        };
        let Ok(document) = Document::parse_with_options(text, parsing()) else {
            return true;
        };
        // usvg reads the embedded documents one after another, on the one thread of the guest:
        // nothing takes a share between this load and the store below.
        let left = self.left.load(Ordering::Relaxed);
        let count = expanded_elements(&document, left);
        if count > left {
            self.exceeded.store(true, Ordering::Relaxed);
            return false;
        }
        self.left.store(left - count, Ordering::Relaxed);
        true
    }
}

/// Whether usvg's default resolver reads a `data:` URL of `mime` as an SVG
/// (`ImageHrefResolver::default_data_resolver`, `src/parser/image.rs:58-76`): `image/svg+xml`
/// always, `text/plain` unless it is a raster picture - which is never XML, so the parse in
/// [`Remaining::take_svg`] tells it.
#[must_use]
pub fn read_as_svg(mime: &str) -> bool {
    matches!(mime, "image/svg+xml" | "text/plain")
}

/// The elements of `doc` as usvg builds them - every `<use>` as what it references, every
/// marker as often as it is drawn, every pattern as often as it is copied (`expand.rs`) -
/// up to `limit + 1`: a result above `limit` means "too many".
#[must_use]
pub fn expanded_elements(doc: &Document, limit: u64) -> u64 {
    expand::count(doc, limit)
}

/// The id a reference `#id` names, as `svgtypes::IRI::from_str` 0.16 reads it
/// (`src/funciri.rs:23-32`, `Stream::parse_iri` at 65-73): spaces before the `#` skipped, the id
/// up to the first plain space (a tab stays part of it), nothing but spaces after it; `None` for
/// an empty id or anything else.
#[must_use]
pub fn iri(value: &str) -> Option<&str> {
    let rest = value.trim_start_matches(SPACES).strip_prefix('#')?;
    let (id, tail) = rest.split_at(rest.find(' ').unwrap_or(rest.len()));
    (!id.is_empty() && tail.trim_start_matches(SPACES).is_empty()).then_some(id)
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
