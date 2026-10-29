//! The `tree` output: the top level, then any collection's children on expansion.
//!
//! A single document shows its root's children at the top (or one `value` node for a scalar
//! root); a stream of several shows `document 1`, `document 2`, ... A mapping's children are its
//! keys, a sequence's `[0]`, `[1]`, ...; a scalar, alias, flow collection or block scalar is a
//! value, shortened; a collection's value is `{n}` or `[n]`. Anchors and aliases are shown as
//! written, never resolved, so no cycle and no expansion bomb can reach the tree.
//!
//! A collection node's id is its first child's offset with the byte column of that child above
//! bit 40, so it costs no memory; a node without children has [`LEAF_BIT`] set. More than
//! [`PAGE`] children (or documents) are shown as ranges (`[0..999]`, ...), nested so no level
//! holds more than [`PAGE`] nodes; a range is the one node kept in a table, up to
//! [`MAX_RANGES`]. A file that is not well formed adds a first node naming the line and column of
//! its first error; what came before it is listed as usual.

use std::collections::HashMap;

use crate::events::EventKind;
use crate::structure::{PAGE, Structure};
use crate::types::{Failure, FailureKind, Source, TreeNode};
use crate::walk::{Shape, Walker};

/// The id of the node naming the first error.
pub const ERROR_NODE: u64 = u64::MAX;
/// The most range nodes one document hands out.
pub const MAX_RANGES: usize = 1 << 16;
/// Marks the id of a node without children.
pub const LEAF_BIT: u64 = 1 << 62;

/// Marks a range id.
const RANGE_BIT: u64 = 1 << 63;
/// The bits of an offset in an id, and where the column starts.
const OFFSET_BITS: u32 = 40;
/// The bits of a column in an id.
const COL_BITS: u32 = 22;

/// What a range lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Container {
    Documents,
    Collection { start: u64, col: u32, seq: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Range {
    container: Container,
    lo: u64,
    hi: u64,
}

/// A child being read: its index, where it starts, its label, its value once known.
struct Draft {
    index: u64,
    start: u64,
    label: String,
    shape: Option<Shape>,
}

#[derive(Debug, Default)]
pub(crate) struct Tree {
    ranges: Vec<Range>,
    ids: HashMap<Range, u64>,
}

impl Tree {
    /// The children of `node`, or the top level for `None`.
    pub(crate) fn children<S: Source>(
        &mut self,
        src: &mut S,
        structure: &Structure,
        node: Option<u64>,
    ) -> Result<Vec<TreeNode>, Failure> {
        let Some(id) = node else {
            return self.top(src, structure);
        };
        if id == ERROR_NODE || (id & LEAF_BIT != 0 && id & RANGE_BIT == 0) {
            return Ok(Vec::new());
        }
        if id & RANGE_BIT != 0 {
            let range = usize::try_from(id & !RANGE_BIT)
                .ok()
                .and_then(|index| self.ranges.get(index))
                .copied()
                .ok_or_else(|| Failure::of(FailureKind::NotFound))?;
            return self.list(src, structure, range);
        }
        let (start, col) = (id & ((1 << OFFSET_BITS) - 1), (id >> OFFSET_BITS) as u32);
        let mut walker = Walker::at_node(src, structure, start, col);
        let Some(first) = walker.next()?.filter(|first| first.start == start) else {
            return Ok(Vec::new());
        };
        match walker.value(&first)? {
            Shape::Collection {
                start,
                col,
                seq,
                count,
            } => self.list(
                src,
                structure,
                Range {
                    container: Container::Collection { start, col, seq },
                    lo: 0,
                    hi: count,
                },
            ),
            Shape::Empty | Shape::Leaf { .. } => Ok(Vec::new()),
        }
    }

    fn top<S: Source>(
        &mut self,
        src: &mut S,
        structure: &Structure,
    ) -> Result<Vec<TreeNode>, Failure> {
        let mut nodes = Vec::new();
        if let Some(error) = &structure.error {
            nodes.push(TreeNode {
                id: ERROR_NODE,
                label: "invalid YAML".to_owned(),
                value: Some(format!(
                    "line {}, column {}: {}",
                    error.line, error.column, error.message
                )),
                has_children: false,
            });
        }
        let count = structure.documents.count;
        if count > 1 {
            nodes.extend(self.list(
                src,
                structure,
                Range {
                    container: Container::Documents,
                    lo: 0,
                    hi: count,
                },
            )?);
            return Ok(nodes);
        }
        let Some(Draft {
            start,
            shape: Some(shape),
            ..
        }) = documents(src, structure, 0, count)?.pop()
        else {
            return Ok(nodes);
        };
        match shape {
            Shape::Collection {
                start,
                col,
                seq,
                count,
            } => nodes.extend(self.list(
                src,
                structure,
                Range {
                    container: Container::Collection { start, col, seq },
                    lo: 0,
                    hi: count,
                },
            )?),
            Shape::Leaf { preview } => nodes.push(TreeNode {
                id: LEAF_BIT | start,
                label: "value".to_owned(),
                value: Some(preview),
                has_children: false,
            }),
            Shape::Empty => {}
        }
        Ok(nodes)
    }

    /// Children `lo..hi` of a container: themselves, or ranges of them when there are too many.
    fn list<S: Source>(
        &mut self,
        src: &mut S,
        structure: &Structure,
        range: Range,
    ) -> Result<Vec<TreeNode>, Failure> {
        let Range { lo, hi, .. } = range;
        let total = hi.saturating_sub(lo);
        if total <= PAGE {
            let drafts = match range.container {
                Container::Documents => documents(src, structure, lo, hi)?,
                Container::Collection { start, col, seq } => {
                    items(src, structure, (start, col, seq), lo, hi)?
                }
            };
            return Ok(drafts.into_iter().map(node).collect());
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
                    lo: start,
                    hi: end,
                    ..range
                })?,
                label: format!("[{start}..{}]", end - 1),
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
                "01a15939-8404-762d-9569-7e4ec903238e.too_many_nodes",
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

/// The id of the collection whose first child starts at `start`, byte column `col`; `None` past
/// what an id holds.
fn collection_id(start: u64, col: u32) -> Option<u64> {
    (start < 1 << OFFSET_BITS && col < 1 << COL_BITS)
        .then_some(start | (u64::from(col) << OFFSET_BITS))
}

fn node(draft: Draft) -> TreeNode {
    let Draft {
        start,
        label,
        shape,
        ..
    } = draft;
    let (id, value, has_children) = match shape.unwrap_or(Shape::Empty) {
        Shape::Empty => (LEAF_BIT | start, None, false),
        Shape::Leaf { preview } => (LEAF_BIT | start, Some(preview), false),
        Shape::Collection {
            start: first,
            col,
            seq,
            count,
        } => {
            let value = Some(if seq {
                format!("[{count}]")
            } else {
                format!("{{{count}}}")
            });
            match collection_id(first, col) {
                Some(id) => (id, value, count > 0),
                None => (LEAF_BIT | start, value, false),
            }
        }
    };
    TreeNode {
        id,
        label,
        value,
        has_children,
    }
}

/// Drafts `lo..hi` of the collection whose first child starts at `start`, byte column `col`: from
/// the nearest mark at or before `lo`, or from the first child.
fn items<S: Source>(
    src: &mut S,
    structure: &Structure,
    (start, col, seq): (u64, u32, bool),
    lo: u64,
    hi: u64,
) -> Result<Vec<Draft>, Failure> {
    let (offset, mut index) = structure
        .entry(start)
        .and_then(|entry| entry.mark(lo))
        .unwrap_or((start, 0));
    let mut walker = Walker::at_node(src, structure, offset, col);
    let mut drafts = Vec::new();
    let mut draft: Option<Draft> = None;
    while let Some(event) = walker.next()? {
        let at_col = event.col == col;
        let kind = event.kind;
        let child = at_col
            && matches!(
                (seq, kind),
                (false, EventKind::Key) | (true, EventKind::Item)
            );
        if child {
            keep(&mut drafts, draft.take(), lo);
            if index >= hi {
                break;
            }
            let label = if seq {
                format!("[{index}]")
            } else {
                event.text.clone()
            };
            draft = Some(Draft {
                index,
                start: event.start,
                label,
                shape: None,
            });
            index += 1;
            continue;
        }
        let ends =
            event.col < col || !matches!(kind, EventKind::Key | EventKind::Item | EventKind::Value);
        // At the child's column only an item may follow a key: its indentless sequence.
        let indentless = at_col && !seq && kind == EventKind::Item;
        if ends || (at_col && !indentless) {
            break;
        }
        if let Some(open) = draft.as_mut().filter(|open| open.shape.is_none()) {
            open.shape = Some(walker.value(&event)?);
        } else if at_col {
            break;
        }
    }
    keep(&mut drafts, draft.take(), lo);
    Ok(drafts)
}

/// Drafts `lo..hi` of the documents: from the nearest mark at or before `lo`, or from the start.
fn documents<S: Source>(
    src: &mut S,
    structure: &Structure,
    lo: u64,
    hi: u64,
) -> Result<Vec<Draft>, Failure> {
    let (offset, mut index) = structure.documents.mark(lo).unwrap_or((structure.start, 0));
    let mut walker = Walker::at_line(src, structure, offset);
    let mut drafts = Vec::new();
    let mut draft: Option<Draft> = None;
    let mut open = false;
    while let Some(event) = walker.next()? {
        match event.kind {
            EventKind::Invalid(_) => break,
            EventKind::DocEnd => {
                keep(&mut drafts, draft.take(), lo);
                open = false;
                continue;
            }
            EventKind::DocStart => open = false,
            EventKind::Key | EventKind::Item | EventKind::Value => {}
        }
        if !open {
            keep(&mut drafts, draft.take(), lo);
            if index >= hi {
                break;
            }
            draft = Some(Draft {
                index,
                start: event.start,
                label: format!("document {}", index + 1),
                shape: None,
            });
            index += 1;
            open = true;
            if event.kind == EventKind::DocStart {
                continue;
            }
        }
        if let Some(root) = draft.as_mut().filter(|root| root.shape.is_none()) {
            root.shape = Some(walker.value(&event)?);
        }
    }
    keep(&mut drafts, draft.take(), lo);
    Ok(drafts)
}

/// Keeps `draft` when it is one of those asked for.
fn keep(drafts: &mut Vec<Draft>, draft: Option<Draft>, lo: u64) {
    if let Some(draft) = draft.filter(|draft| draft.index >= lo) {
        drafts.push(draft);
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
