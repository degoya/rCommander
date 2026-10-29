//! The `tree` output: the sections and tables of the [`Outline`] as nodes, the entries as leaves
//! labelled with their key and valued with their value cut short.
//!
//! A level lists its entries first, then its tables (for the top level: the entries before the
//! first header, then the sections). An entry's id is its number, a node's its index, so neither
//! costs memory; an entry is read from the nearest checkpoint before it when its level is opened.
//! More than [`PAGE`] children are shown as ranges (`items 1..1000`, ...), nested so no level
//! holds more than [`PAGE`] nodes, as the CSV viewer does; a range is the one node kept in a
//! table, up to [`MAX_RANGES`] of them.

use std::collections::HashMap;

use rc_plugin_text::Encoding;

use crate::entry::{LineKind, entry, line_kind};
use crate::lexer::{LineState, lex_line};
use crate::lines::LineIndex;
use crate::outline::Outline;
use crate::reader::{Newline, Reader};
use crate::text::decode_line;
use crate::tokens::{LINE_CHUNK, MAX_LINE_BYTES};
use crate::types::{Failure, FailureKind, Format, Source, TreeNode};

/// The most nodes one level shows.
pub const PAGE: u64 = 1000;
/// The most range nodes one document hands out.
pub const MAX_RANGES: usize = 1 << 16;
/// The characters a label or value is cut to.
pub const PREVIEW_CHARS: usize = 80;

/// Marks a range id.
const RANGE_BIT: u64 = 1 << 63;
/// Marks an entry id: the entry's number below it.
const ENTRY_BIT: u64 = 1 << 62;
/// The one node that says how many sections the tree leaves out.
const HIDDEN_ID: u64 = 1 << 61;

/// What the tree reads the document with.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Doc<'a> {
    pub(crate) format: Format,
    pub(crate) encoding: Encoding,
    pub(crate) lines: &'a LineIndex,
    pub(crate) outline: &'a Outline,
}

/// Children `lo..hi` of a level: the top level (`parent` `None`) or a node's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Range {
    parent: Option<u32>,
    lo: u64,
    hi: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Tree {
    ranges: Vec<Range>,
    ids: HashMap<Range, u64>,
}

/// A level's entries and tables.
struct Level<'o> {
    first_entry: u64,
    entries: u64,
    nodes: &'o [u32],
    /// The top level with sections left out: one more child saying so.
    hidden: u64,
}

impl Level<'_> {
    fn len(&self) -> u64 {
        self.entries + self.nodes.len() as u64 + u64::from(self.hidden > 0)
    }
}

fn level(outline: &Outline, parent: Option<u32>) -> Option<Level<'_>> {
    match parent {
        None => Some(Level {
            first_entry: 0,
            entries: outline.root_entries,
            nodes: &outline.top,
            hidden: outline.hidden,
        }),
        Some(index) => outline.node(index).map(|node| Level {
            first_entry: node.first_entry,
            entries: node.entries,
            nodes: &node.children,
            hidden: 0,
        }),
    }
}

impl Tree {
    /// The children of `node`, or the top level for `None`.
    pub(crate) fn children<S: Source>(
        &mut self,
        src: &mut S,
        doc: Doc<'_>,
        node: Option<u64>,
    ) -> Result<Vec<TreeNode>, Failure> {
        let range = match node {
            None => self.whole(doc, None)?,
            Some(id) if id & RANGE_BIT != 0 => usize::try_from(id & !RANGE_BIT)
                .ok()
                .and_then(|index| self.ranges.get(index))
                .copied()
                .ok_or_else(|| Failure::of(FailureKind::NotFound))?,
            Some(id) if id & ENTRY_BIT != 0 || id == HIDDEN_ID => return Ok(Vec::new()),
            Some(id) => {
                let index = u32::try_from(id).map_err(|_| Failure::of(FailureKind::NotFound))?;
                self.whole(doc, Some(index))?
            }
        };
        self.list(src, doc, range)
    }

    /// All children of a level.
    fn whole(&self, doc: Doc<'_>, parent: Option<u32>) -> Result<Range, Failure> {
        let level = level(doc.outline, parent).ok_or_else(|| Failure::of(FailureKind::NotFound))?;
        Ok(Range {
            parent,
            lo: 0,
            hi: level.len(),
        })
    }

    /// Children `lo..hi`: themselves, or ranges of them when there are too many.
    fn list<S: Source>(
        &mut self,
        src: &mut S,
        doc: Doc<'_>,
        range: Range,
    ) -> Result<Vec<TreeNode>, Failure> {
        let Range { parent, lo, hi } = range;
        let total = hi.saturating_sub(lo);
        if total <= PAGE {
            return items(src, doc, parent, lo, hi);
        }
        let mut step = PAGE;
        while total.div_ceil(step) > PAGE {
            step = step.saturating_mul(PAGE);
        }
        let mut nodes = Vec::new();
        let mut start = lo;
        while start < hi {
            let end = start.saturating_add(step).min(hi);
            nodes.push(TreeNode {
                id: self.range_id(Range {
                    parent,
                    lo: start,
                    hi: end,
                })?,
                label: format!("items {}..{end}", start + 1),
                value: None,
                has_children: true,
            });
            start = end;
        }
        Ok(nodes)
    }

    fn range_id(&mut self, range: Range) -> Result<u64, Failure> {
        if let Some(id) = self.ids.get(&range) {
            return Ok(*id);
        }
        if self.ranges.len() >= MAX_RANGES {
            return Err(Failure::coded(
                FailureKind::Other,
                "01a159c1-6b39-7240-9bbb-81ecaf343ed1.too_many_nodes",
            ));
        }
        let id = RANGE_BIT | self.ranges.len() as u64;
        self.ranges.push(range);
        self.ids.insert(range, id);
        Ok(id)
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.ranges.capacity() * size_of::<Range>()
            + self.ids.capacity() * (size_of::<Range>() + size_of::<u64>() + 1)
    }
}

/// Children `lo..hi` of a level themselves: its entries in that span, then its nodes.
fn items<S: Source>(
    src: &mut S,
    doc: Doc<'_>,
    parent: Option<u32>,
    lo: u64,
    hi: u64,
) -> Result<Vec<TreeNode>, Failure> {
    let level = level(doc.outline, parent).ok_or_else(|| Failure::of(FailureKind::NotFound))?;
    let mut nodes = if lo < level.entries {
        let first = level.first_entry;
        entries(src, doc, first + lo, first + hi.min(level.entries))?
    } else {
        Vec::new()
    };
    for index in lo.max(level.entries)..hi {
        let at = usize::try_from(index - level.entries).unwrap_or(usize::MAX);
        match level.nodes.get(at) {
            Some(&node) => nodes.extend(table(doc.outline, node)),
            None if level.hidden > 0 => nodes.push(TreeNode {
                id: HIDDEN_ID,
                label: format!("{} more sections not shown", level.hidden),
                value: None,
                has_children: false,
            }),
            None => {}
        }
    }
    Ok(nodes)
}

/// The node of section or table `index`.
fn table(outline: &Outline, index: u32) -> Option<TreeNode> {
    outline.node(index).map(|node| TreeNode {
        id: u64::from(index),
        label: node.label.clone(),
        value: None,
        has_children: node.entries > 0 || !node.children.is_empty(),
    })
}

/// Entries `lo..hi` of the document, read from the nearest checkpoint before the first.
fn entries<S: Source>(
    src: &mut S,
    doc: Doc<'_>,
    lo: u64,
    hi: u64,
) -> Result<Vec<TreeNode>, Failure> {
    let point = doc.lines.before_entry(lo);
    let mut reader = Reader::new(src, point.offset, Newline::of(doc.encoding), LINE_CHUNK);
    let (mut state, mut number) = (point.state, point.entries);
    let (mut raw, mut text, mut tokens) = (Vec::new(), String::new(), Vec::new());
    let mut nodes = Vec::new();
    while number < hi {
        let Some(capped) = reader.read_line(MAX_LINE_BYTES, &mut raw)? else {
            break;
        };
        text.clear();
        decode_line(&raw, capped, doc.encoding, &mut text);
        tokens.clear();
        let before = state;
        state = lex_line(text.as_bytes(), state, doc.format, Some(&mut tokens));
        if line_kind(&text, before, &tokens) != LineKind::Entry {
            continue;
        }
        if number >= lo {
            let (label, value) = entry(&text, &tokens, state != LineState::Clear);
            nodes.push(TreeNode {
                id: ENTRY_BIT | number,
                label,
                value,
                has_children: false,
            });
        }
        number += 1;
    }
    Ok(nodes)
}

/// `text` on one line, cut at [`PREVIEW_CHARS`]; `cut` adds the ellipsis even when it fits.
pub(crate) fn preview(text: &str, cut: bool) -> String {
    let mut out = String::new();
    for (count, character) in text.chars().enumerate() {
        if count == PREVIEW_CHARS {
            out.push('\u{2026}');
            return out;
        }
        out.push(if character.is_control() {
            ' '
        } else {
            character
        });
    }
    if cut {
        out.push('\u{2026}');
    }
    out
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
