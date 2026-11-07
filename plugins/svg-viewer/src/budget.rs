//! The `<use>` budget: a few kilobytes of `<use>` elements, each repeating a group of the ones
//! before it ten times, expand into billions of elements - the SVG form of the billion laughs.
//! usvg builds every copy, so the document is counted first, in one pass over its elements, as
//! if each `<use>` were replaced by what it references, and refused beyond the budget. An SVG
//! inside a `data:` URL, which usvg reads while it builds the picture, takes its share from what
//! the document left ([`Remaining`]), once each time usvg reads it (RC-100-76).
//!
//! The count follows usvg 0.49's own rules (`usvg-0.49.0/src/parser/svgtree/parse.rs`): the ids
//! as it maps them, the `href` it follows, the reference as `svgtypes` reads it; and since
//! RC-100-77 the markers and patterns it instantiates (`expand.rs`). Since RC-100-78 the same
//! pass counts the cubic curves kurbo cuts the arcs into ([`MAX_ARC_CURVES`],
//! [`MAX_CURVES_PER_ARC`], `segments.rs`). What resvg then draws of the patterns and into its
//! intermediate canvases has a budget of its own, in pixels (`pixels.rs`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use roxmltree::{Document, ParsingOptions};

use crate::depth::{MAX_DEPTH, tree_within};
use crate::expand;

/// The most elements a document may have with its `<use>` references expanded: far beyond a
/// diagram or a map, far below what fills the store's memory.
pub const MAX_EXPANDED_ELEMENTS: u64 = 1 << 20;

/// The most cubic curves kurbo may cut the arcs of one picture into, every element as often as
/// usvg builds it: every `A` of a `d`, the four quarter arcs of every circle, ellipse and rounded
/// rect, and the path a `textPath` follows (RC-100-78). An ordinary arc makes one to five, so
/// this is as far beyond a drawing as the element budget, and each curve costs tiny-skia some
/// 25 bytes and the drawing its flattening.
pub const MAX_ARC_CURVES: u64 = 1 << 20;

/// The most cubic curves one `A` of a `d` may make. svgtypes 0.16 buffers an arc's curves and
/// hands them out one by one with `Vec::remove(0)` (`SimplifyingPathParser`, `svgtypes-0.16.2/
/// src/path.rs:753-756, 960-990, 1051`), so an arc costs the square of its curves; kurbo makes 64
/// of a whole ellipse of radius 6e9 user units, far beyond a drawing. With [`MAX_ARC_CURVES`] the
/// buffer moves at most 32 curves per curve.
pub const MAX_CURVES_PER_ARC: u64 = 1 << 6;

/// What usvg builds of a document: its elements, `<use>`, markers and patterns expanded, and the
/// cubic curves of its arcs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    pub elements: u64,
    pub curves: u64,
}

impl Cost {
    /// The budget of one picture, the SVGs it embeds included.
    pub const LIMIT: Self = Self {
        elements: MAX_EXPANDED_ELEMENTS,
        curves: MAX_ARC_CURVES,
    };

    /// Whether either part is beyond `limit`.
    #[must_use]
    pub fn exceeds(self, limit: Self) -> bool {
        self.elements > limit.elements || self.curves > limit.curves
    }
}

/// What `svgtypes` 0.16 skips as spaces (`Stream::skip_spaces`, `src/stream.rs:49-51`).
const SPACES: [char; 4] = [' ', '\t', '\n', '\r'];

/// The most nodes roxmltree builds of one document - each element and the text after it, and the
/// root: the element budget, held while the document is parsed (RC-080-69), not only counted
/// after it.
pub const MAX_PARSED_NODES: u32 = (1 << 21) + 1;

/// How a document is parsed here: without DTDs (`crate::doctype` blanks the declaration first;
/// RC-080-69) and with at most [`MAX_PARSED_NODES`] nodes.
#[must_use]
pub fn parsing<'input>() -> ParsingOptions<'input> {
    ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_PARSED_NODES,
        ..ParsingOptions::default()
    }
}

/// The part of the budget the document left for the SVGs usvg reads from `data:` URLs while it
/// builds the picture. Once a share does not fit, it stays spent: every later one is refused, and
/// the picture is refused as too complex.
#[derive(Debug)]
pub struct Remaining {
    elements: AtomicU64,
    curves: AtomicU64,
    exceeded: AtomicBool,
}

impl Remaining {
    #[must_use]
    pub fn new(left: Cost) -> Self {
        Self {
            elements: AtomicU64::new(left.elements),
            curves: AtomicU64::new(left.curves),
            exceeded: AtomicBool::new(false),
        }
    }

    /// Whether a share did not fit.
    #[must_use]
    pub fn exceeded(&self) -> bool {
        self.exceeded.load(Ordering::Relaxed)
    }

    /// Whether usvg may read `data` (its DOCTYPE blanked, `crate::doctype`) as the SVG it would
    /// make of it, its elements and arc curves taken from what is left. Bytes that are no UTF-8 or no XML take
    /// nothing: usvg fails on them as well (`Tree::from_data`, `src/parser/mod.rs:105-120`). A
    /// DTD the blanking missed, more than [`MAX_PARSED_NODES`] nodes or a tree nested beyond
    /// [`MAX_DEPTH`] is not read: usvg would read them without these limits (RC-080-69).
    pub fn take_svg(&self, data: &[u8]) -> bool {
        if self.exceeded() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            return true;
        };
        let document = match Document::parse_with_options(text, parsing()) {
            Ok(document) => document,
            Err(roxmltree::Error::DtdDetected) => return false,
            Err(roxmltree::Error::NodesLimitReached) => {
                self.exceeded.store(true, Ordering::Relaxed);
                return false;
            }
            Err(_) => return true,
        };
        if !tree_within(&document, MAX_DEPTH) {
            return false;
        }
        // usvg reads the embedded documents one after another, on the one thread of the guest:
        // nothing takes a share between this load and the store below.
        let left = Cost {
            elements: self.elements.load(Ordering::Relaxed),
            curves: self.curves.load(Ordering::Relaxed),
        };
        let count = expanded(&document, left);
        if count.exceeds(left) {
            self.exceeded.store(true, Ordering::Relaxed);
            return false;
        }
        self.elements
            .store(left.elements - count.elements, Ordering::Relaxed);
        self.curves
            .store(left.curves - count.curves, Ordering::Relaxed);
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
/// marker as often as it is drawn, every pattern as often as it is copied (`expand.rs`) - and
/// the cubic curves of the arcs of those elements, each part up to its `limit + 1`: a result
/// that [`Cost::exceeds`] `limit` means "too many".
#[must_use]
pub fn expanded(doc: &Document, limit: Cost) -> Cost {
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
