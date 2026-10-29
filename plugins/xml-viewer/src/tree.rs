//! The `tree` output: the root element, then any element's child elements on expansion.
//!
//! A node is an element: its label the start tag cut short (`<item id="7" ...>`), its value the
//! text of an element without child elements. Text, comments and processing instructions are no
//! nodes. A node's id is the offset of its `<` in the file, so an element node costs no memory. An
//! element of more than [`PAGE`] children is shown as ranges (`[0..999]`, ...), nested so no level
//! holds more than [`PAGE`] nodes; a range is the one node kept in a table, up to [`MAX_RANGES`]
//! of them. A file that is not well-formed adds a first node naming the line and column of its
//! first error; what came before it is listed as usual.

use std::collections::HashMap;

use crate::lexer::Kind;
use crate::structure::{PAGE, Structure};
use crate::types::{Failure, FailureKind, Source, TreeNode};
use crate::walk::{Element, Walker};

/// The id of the node naming the first error.
pub const ERROR_NODE: u64 = u64::MAX;
/// The most range nodes one document hands out.
pub const MAX_RANGES: usize = 1 << 16;

/// Marks a range id; offsets stay below it.
const RANGE_BIT: u64 = 1 << 63;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Range {
    container: u64,
    lo: u64,
    hi: u64,
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
        match node {
            None => top(src, structure),
            Some(ERROR_NODE) => Ok(Vec::new()),
            Some(id) if id & RANGE_BIT != 0 => {
                let range = usize::try_from(id & !RANGE_BIT)
                    .ok()
                    .and_then(|index| self.ranges.get(index))
                    .copied()
                    .ok_or_else(|| Failure::of(FailureKind::NotFound))?;
                self.list(src, structure, range)
            }
            Some(offset) => {
                let count = {
                    let mut walker = Walker::new(src, structure, offset);
                    match walker.next()? {
                        Some(open) if open.kind == Kind::Open && open.start == offset => {
                            walker.element(&open)?.count
                        }
                        _ => 0,
                    }
                };
                if count == 0 {
                    return Ok(Vec::new());
                }
                self.list(
                    src,
                    structure,
                    Range {
                        container: offset,
                        lo: 0,
                        hi: count,
                    },
                )
            }
        }
    }

    /// Children `lo..hi` of an element: themselves, or ranges of them when there are too many.
    fn list<S: Source>(
        &mut self,
        src: &mut S,
        structure: &Structure,
        range: Range,
    ) -> Result<Vec<TreeNode>, Failure> {
        let Range { container, lo, hi } = range;
        let total = hi.saturating_sub(lo);
        if total <= PAGE {
            return items(src, structure, range);
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
                    container,
                    lo: start,
                    hi: end,
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
                "01a15939-9062-79b4-85e8-46e1ed70ad4f.too_many_nodes",
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

/// The top level: the first error, if any, and the root element.
fn top<S: Source>(src: &mut S, structure: &Structure) -> Result<Vec<TreeNode>, Failure> {
    let mut nodes = Vec::new();
    if let Some(error) = &structure.error {
        nodes.push(TreeNode {
            id: ERROR_NODE,
            label: "invalid XML".to_owned(),
            value: Some(format!(
                "line {}, column {}: {}",
                error.line, error.column, error.message
            )),
            has_children: false,
        });
    }
    let Some(root) = structure.root else {
        return Ok(nodes);
    };
    let mut walker = Walker::new(src, structure, root);
    if let Some(open) = walker.next()?
        && open.kind == Kind::Open
    {
        nodes.push(node(root, walker.element(&open)?));
    }
    Ok(nodes)
}

/// Child elements `lo..hi` themselves, at most [`PAGE`] of them: from the nearest anchor at or
/// before `lo`, or from the element's content.
fn items<S: Source>(
    src: &mut S,
    structure: &Structure,
    range: Range,
) -> Result<Vec<TreeNode>, Failure> {
    let Range { container, lo, hi } = range;
    let entry = structure.entry(container);
    let anchored = entry.and_then(|entry| {
        let slot = usize::try_from(lo / entry.anchor_step)
            .ok()?
            .checked_sub(1)?;
        let offset = *entry.anchors.get(slot)?;
        Some((offset, (slot as u64 + 1) * entry.anchor_step))
    });
    let (offset, mut index) = match anchored {
        Some(found) => found,
        None => (entry.map_or(container, |entry| entry.content), 0),
    };
    let mut walker = Walker::new(src, structure, offset);
    if anchored.is_none() && entry.is_none() {
        // A small element: its start tag first.
        match walker.next()? {
            Some(open) if open.kind == Kind::Open && open.start == container => {
                if !walker.skip_head()? {
                    return Ok(Vec::new());
                }
            }
            _ => return Ok(Vec::new()),
        }
    }
    let mut nodes = Vec::new();
    while index < hi {
        let Some(lexeme) = walker.next()? else {
            break;
        };
        match lexeme.kind {
            Kind::Open => {
                let element = walker.element(&lexeme)?;
                if index >= lo {
                    nodes.push(node(lexeme.start, element));
                }
                index += 1;
            }
            // The element's own end tag.
            Kind::EndOpen => break,
            _ => {}
        }
    }
    Ok(nodes)
}

fn node(id: u64, element: Element) -> TreeNode {
    TreeNode {
        id,
        label: element.label,
        value: element.text,
        has_children: element.count > 0,
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
