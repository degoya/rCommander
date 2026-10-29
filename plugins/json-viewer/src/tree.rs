//! The `tree` output: the root's children, then any node's on expansion.
//!
//! A node's id is the offset of its value in the file, so a value node costs no memory. A
//! container of more than [`PAGE`] children is shown as ranges (`[0..999]`, ...), nested so no
//! level holds more than [`PAGE`] nodes; a range is the one node kept in a table, up to
//! [`MAX_RANGES`] of them. An invalid file adds a first node naming the line and column of its
//! first error; what came before it is listed as usual.

use std::collections::HashMap;

use crate::structure::{PAGE, Structure};
use crate::types::{Failure, FailureKind, Source, TreeNode};
use crate::walk::{Shape, Walker, key_label};

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
            None => self.top(src, structure),
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
                let shape = Walker::new(src, structure, offset)
                    .value()?
                    .filter(|(start, _)| *start == offset);
                match shape {
                    Some((_, Shape::Container { count, .. })) => self.list(
                        src,
                        structure,
                        Range {
                            container: offset,
                            lo: 0,
                            hi: count,
                        },
                    ),
                    _ => Ok(Vec::new()),
                }
            }
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
                label: "invalid JSON".to_owned(),
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
        let value = Walker::new(src, structure, root).value()?;
        match value {
            Some((_, Shape::Container { count, .. })) => nodes.extend(self.list(
                src,
                structure,
                Range {
                    container: root,
                    lo: 0,
                    hi: count,
                },
            )?),
            Some((_, Shape::Scalar { preview })) => nodes.push(TreeNode {
                id: root,
                label: "value".to_owned(),
                value: Some(preview),
                has_children: false,
            }),
            None => {}
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
                "01a1588a-5ec2-7323-9d13-2eaae3eef934.too_many_nodes",
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

/// Children `lo..hi` themselves, at most [`PAGE`] of them: from the nearest anchor at or before
/// `lo`, or from the container's start.
fn items<S: Source>(
    src: &mut S,
    structure: &Structure,
    range: Range,
) -> Result<Vec<TreeNode>, Failure> {
    let Range { container, lo, hi } = range;
    let anchored = structure.entry(container).and_then(|entry| {
        let slot = usize::try_from(lo / entry.anchor_step)
            .ok()?
            .checked_sub(1)?;
        let offset = *entry.anchors.get(slot)?;
        Some((offset, (slot as u64 + 1) * entry.anchor_step))
    });
    let object = src.read_at(container, 1)?.first() == Some(&b'{');
    let (offset, mut index) = anchored.unwrap_or((container + 1, 0));
    let mut walker = Walker::new(src, structure, offset);
    let mut nodes = Vec::new();
    while index < hi {
        match walker.peek()? {
            Some(b',') => {
                walker.next()?;
                continue;
            }
            Some(b'}' | b']') | None => break,
            Some(_) => {}
        }
        let (label, key_start) = if object {
            let Some(key) = walker.next()? else { break };
            let whole = key.end - key.start == walker.captured().len() as u64;
            let label = key_label(walker.captured(), whole);
            match walker.next()? {
                Some(colon) if colon.kind == crate::lexer::Kind::Colon => {}
                _ => {
                    // The valid part ends between a key and its value.
                    if index >= lo {
                        nodes.push(TreeNode {
                            id: key.start,
                            label,
                            value: None,
                            has_children: false,
                        });
                    }
                    break;
                }
            }
            (label, Some(key.start))
        } else {
            (format!("[{index}]"), None)
        };
        let Some((start, shape)) = walker.value()? else {
            if index >= lo {
                nodes.push(TreeNode {
                    id: key_start.unwrap_or(container),
                    label,
                    value: None,
                    has_children: false,
                });
            }
            break;
        };
        if index >= lo {
            nodes.push(node(start, label, shape));
        }
        index += 1;
    }
    Ok(nodes)
}

fn node(id: u64, label: String, shape: Shape) -> TreeNode {
    match shape {
        Shape::Container { object, count } => TreeNode {
            id,
            label,
            value: Some(if object {
                format!("{{{count}}}")
            } else {
                format!("[{count}]")
            }),
            has_children: count > 0,
        },
        Shape::Scalar { preview } => TreeNode {
            id,
            label,
            value: Some(preview),
            has_children: false,
        },
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
