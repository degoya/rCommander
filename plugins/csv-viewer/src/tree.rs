//! The `tree` output: a node per record, `row <n>` counted from the first record after the
//! header, its value the first fields cut short; its children the fields, each labelled with its
//! column's name from the header (`column <n>` without one) and valued with the field cut short.
//!
//! A record's id is its number, a field's the record's and the column's, so neither costs memory.
//! More than [`PAGE`] records (or fields of one record) are shown as ranges (`rows 1..1000`, ...),
//! nested so no level holds more than [`PAGE`] nodes, as the JSON viewer does; a range is the one
//! node kept in a table, up to [`MAX_RANGES`] of them. A page of records is read from the nearest
//! record checkpoint, never from the file's start.

use std::collections::HashMap;

use crate::encoding::{Encoding, decode};
use crate::lines::RecordIndex;
use crate::types::{Failure, FailureKind, Source, TreeNode};
use crate::walk::Walker;

/// The most nodes one level shows.
pub const PAGE: u64 = 1000;
/// The most range nodes one document hands out.
pub const MAX_RANGES: usize = 1 << 16;
/// The characters a label or value is cut to.
pub const PREVIEW_CHARS: usize = 80;
/// The fields of a record its value shows, at most.
const PREVIEW_FIELDS: u64 = 8;
/// The bytes of a field read for its preview: 80 characters of up to four bytes.
const PREVIEW_BYTES: usize = PREVIEW_CHARS * 4;

/// Marks a range id.
const RANGE_BIT: u64 = 1 << 63;
/// Marks a field id: the record above [`COLUMN_BITS`], the column below.
const FIELD_BIT: u64 = 1 << 62;
const COLUMN_BITS: u32 = 20;
const COLUMN_MASK: u64 = (1 << COLUMN_BITS) - 1;
const RECORD_MASK: u64 = (1 << (62 - COLUMN_BITS)) - 1;

/// What the tree needs of the document.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Table<'a> {
    pub(crate) sep: u8,
    pub(crate) encoding: Encoding,
    pub(crate) records: &'a RecordIndex,
    /// The records of the file before the first row: 1 with a header.
    pub(crate) skip: u64,
    /// The header's fields; empty without one.
    pub(crate) names: &'a [Vec<u8>],
}

impl Table<'_> {
    /// The rows: the records after the header.
    pub(crate) fn rows(&self) -> u64 {
        self.records.count().saturating_sub(self.skip)
    }
}

/// Children `lo..hi` of the rows (`row` `None`) or of one row's fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Range {
    row: Option<u64>,
    lo: u64,
    hi: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Tree {
    ranges: Vec<Range>,
    ids: HashMap<Range, u64>,
}

impl Tree {
    /// The children of `node`, or the rows for `None`.
    pub(crate) fn children<S: Source>(
        &mut self,
        src: &mut S,
        table: &Table<'_>,
        node: Option<u64>,
    ) -> Result<Vec<TreeNode>, Failure> {
        let range = match node {
            None => Range {
                row: None,
                lo: 0,
                hi: table.rows(),
            },
            Some(id) if id & RANGE_BIT != 0 => usize::try_from(id & !RANGE_BIT)
                .ok()
                .and_then(|index| self.ranges.get(index))
                .copied()
                .ok_or_else(|| Failure::of(FailureKind::NotFound))?,
            Some(id) if id & FIELD_BIT != 0 => return Ok(Vec::new()),
            Some(row) if row < table.rows() => {
                let (nodes, count) = fields(src, table, row, 0, PAGE)?;
                if count <= PAGE {
                    return Ok(nodes);
                }
                Range {
                    row: Some(row),
                    lo: 0,
                    hi: count,
                }
            }
            Some(_) => return Err(Failure::of(FailureKind::NotFound)),
        };
        self.list(src, table, range)
    }

    /// Children `lo..hi`: themselves, or ranges of them when there are too many.
    fn list<S: Source>(
        &mut self,
        src: &mut S,
        table: &Table<'_>,
        range: Range,
    ) -> Result<Vec<TreeNode>, Failure> {
        let Range { row, lo, hi } = range;
        let total = hi.saturating_sub(lo);
        if total <= PAGE {
            return match row {
                None => rows(src, table, lo, hi),
                Some(row) => Ok(fields(src, table, row, lo, hi)?.0),
            };
        }
        let mut step = PAGE;
        while total.div_ceil(step) > PAGE {
            step = step.saturating_mul(PAGE);
        }
        let what = if row.is_some() { "columns" } else { "rows" };
        let mut nodes = Vec::new();
        let mut start = lo;
        while start < hi {
            let end = start.saturating_add(step).min(hi);
            nodes.push(TreeNode {
                id: self.range_id(Range {
                    row,
                    lo: start,
                    hi: end,
                })?,
                label: format!("{what} {}..{end}", start + 1),
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
                "01a15984-4cfe-79a8-a86c-38186923b56a.too_many_nodes",
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

/// A walker at the start of record `record` of the file (the header counted).
fn walker_at<'s, S: Source>(
    src: &'s mut S,
    table: &Table<'_>,
    record: u64,
) -> Result<Walker<'s, S>, Failure> {
    let point = table.records.at_or_before(record);
    let mut walker = Walker::new(src, point.offset, table.sep);
    for _ in point.at..record {
        if !walker.skip()? {
            break;
        }
    }
    Ok(walker)
}

/// Rows `lo..hi` themselves.
fn rows<S: Source>(
    src: &mut S,
    table: &Table<'_>,
    lo: u64,
    hi: u64,
) -> Result<Vec<TreeNode>, Failure> {
    let mut walker = walker_at(src, table, lo + table.skip)?;
    let mut nodes = Vec::new();
    for row in lo..hi {
        let mut values = Vec::new();
        let count = walker.record(
            &mut |column| {
                if column < PREVIEW_FIELDS {
                    PREVIEW_BYTES
                } else {
                    0
                }
            },
            &mut |column, value| {
                if column < PREVIEW_FIELDS {
                    values.push(value);
                }
            },
        )?;
        let Some(count) = count else { break };
        let shown: Vec<String> = values
            .iter()
            .map(|value| decode(value, table.encoding))
            .collect();
        let more = count > PREVIEW_FIELDS;
        nodes.push(TreeNode {
            id: row,
            label: format!("row {}", row + 1),
            value: Some(preview(&shown.join(" | "), more)),
            has_children: count > 0,
        });
    }
    Ok(nodes)
}

/// Fields `lo..hi` of row `row`, and how many fields the row has.
fn fields<S: Source>(
    src: &mut S,
    table: &Table<'_>,
    row: u64,
    lo: u64,
    hi: u64,
) -> Result<(Vec<TreeNode>, u64), Failure> {
    let mut walker = walker_at(src, table, row + table.skip)?;
    let mut nodes = Vec::new();
    let wanted = |column: u64| (lo..hi).contains(&column);
    let count = walker.record(
        &mut |column| if wanted(column) { PREVIEW_BYTES } else { 0 },
        &mut |column, value| {
            if wanted(column) {
                nodes.push(TreeNode {
                    id: FIELD_BIT | ((row & RECORD_MASK) << COLUMN_BITS) | column.min(COLUMN_MASK),
                    label: column_name(table, column),
                    value: Some(preview(&decode(&value, table.encoding), false)),
                    has_children: false,
                });
            }
        },
    )?;
    Ok((nodes, count.unwrap_or(0)))
}

/// The header's name of `column`, or `column <n>` when there is none.
fn column_name(table: &Table<'_>, column: u64) -> String {
    let name = usize::try_from(column)
        .ok()
        .and_then(|index| table.names.get(index))
        .map(|name| decode(name, table.encoding))
        .filter(|name| !name.trim().is_empty());
    match name {
        Some(name) => preview(&name, false),
        None => format!("column {}", column + 1),
    }
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
