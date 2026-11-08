//! Rows of a diff the edit mode works along: the blocks of differing rows, the rows the
//! cursor copies, and where lines go in at a row without one on a side (RC-100-94).

use std::ops::Range;

use super::CompareSide;
use crate::compare::text::{RowKind, TextDiff};

/// The index of the line `side` has at `row` or next after it: where lines go in at a row
/// without one on that side.
pub(super) fn next_line(diff: &TextDiff, row: usize, side: CompareSide) -> usize {
    let at = diff.position(row);
    match side {
        CompareSide::Left => at.left - 1,
        CompareSide::Right => at.right - 1,
    }
}

/// The rows of every block of differing rows of `diff`, in order: the blocks a `files_compare`
/// answer counts, which `compare.apply_blocks` names by their place in this list.
#[must_use]
pub fn blocks(diff: &TextDiff) -> Vec<Range<usize>> {
    diff.change_starts()
        .map(|start| block_end(diff, start))
        .collect()
}

/// The rows the cursor at `row` copies: its block of differing rows, or the row itself when it
/// is equal (Total Commander copies the line then).
#[must_use]
pub fn rows_at(diff: &TextDiff, row: usize) -> Range<usize> {
    if row >= diff.rows.len() {
        return row..row;
    }
    if diff.rows[row].kind == RowKind::Equal {
        return row..row + 1;
    }
    let start = diff.rows[..row]
        .iter()
        .rposition(|shown| shown.kind == RowKind::Equal)
        .map_or(0, |equal| equal + 1);
    block_end(diff, start)
}

fn block_end(diff: &TextDiff, start: usize) -> Range<usize> {
    let end = diff.rows[start..]
        .iter()
        .position(|row| row.kind == RowKind::Equal)
        .map_or(diff.rows.len(), |len| start + len);
    start..end
}
