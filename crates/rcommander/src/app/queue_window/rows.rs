//! Which rows of the queue window are built into widgets (RC-080-45): those around the list's
//! scroll offset, however many jobs the queue holds. The room of the others stays as empty space
//! above and below, so the list is as tall as before and the scrollbar and every offset stay
//! where they were. A row is one line, a running job's two: its file and figures under it
//! ([`super::figures::live`]).
//!
//! The list reports its offset when it scrolls, not its height; the rows built cover more than
//! any screen is tall, so a taller window needs no report. Pure arithmetic, tested without a
//! window.

use std::ops::Range;

use rc_engine::{JobSnapshot, JobState};

use crate::theme::ROW_HEIGHT;

/// The space between two rows of the list.
pub const SPACING: f32 = 2.0;
/// The height the rows built cover from the offset down: more than a screen is tall.
const SPAN: f32 = 4800.0;
/// The rows built above the offset too, so a step up shows no empty space.
const OVERSCAN: f32 = 10.0 * ROW_HEIGHT;

/// The rows built and the room of those that are not.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    /// The space above the first row built: the rows before it with their spacing.
    pub above: f32,
    /// The rows built, by index.
    pub rows: Range<usize>,
    /// The space below the last row built.
    pub below: f32,
}

/// The height of `job`'s row: a running job on a file has a second line.
pub fn height(job: &JobSnapshot) -> f32 {
    if job.state == JobState::Running && job.progress.current.is_some() {
        2.0 * ROW_HEIGHT
    } else {
        ROW_HEIGHT
    }
}

/// The rows of `heights` (one per row, in order) to build for the list scrolled to `top`.
pub fn shown(heights: &[f32], top: f32) -> Shown {
    let total: f32 = heights.iter().map(|height| height + SPACING).sum();
    // A list that got shorter since its last scroll is shown from where it can still be.
    let top = top.min((total - SPAN).max(0.0));
    let (from, to) = (top - OVERSCAN, top + SPAN);
    let (mut above, mut y) = (0.0, 0.0);
    let (mut first, mut end) = (heights.len(), heights.len());
    for (index, height) in heights.iter().enumerate() {
        let bottom = y + height;
        if first == heights.len() && bottom >= from {
            first = index;
            above = y;
        }
        if y > to {
            end = index;
            break;
        }
        y = bottom + SPACING;
    }
    let first = first.min(end);
    let built: f32 = heights[first..end]
        .iter()
        .map(|height| height + SPACING)
        .sum();
    Shown {
        above,
        rows: first..end,
        below: (total - above - built).max(0.0),
    }
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
