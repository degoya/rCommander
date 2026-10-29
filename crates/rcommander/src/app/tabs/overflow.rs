//! A tab row that does not fit its panel (`docs/design.md`, Bars that do not fit; RC-010-21): as
//! many tabs as fit stay, in their order, the active one always among them, and `»` after them
//! lists every tab; no tab is cut off. As the drive bar (`drive_bar/overflow.rs`), the widths are
//! estimated from the font, here the list's (RC-010-17), as the view holds no renderer.

use std::ops::Range;

use crate::theme::fonts::AreaFont;

/// The space between two tabs.
pub(super) const SPACING: f32 = 2.0;
/// A tab's padding on each side.
pub(super) const PADDING: f32 = 12.0;

/// The width of a tab showing `title` in `font`.
pub(super) fn tab_width(title: &str, font: AreaFont) -> f32 {
    font.width(title) + 2.0 * PADDING
}

/// Which of the tabs `widths` wide are shown in `available` pixels, [`SPACING`] apart: all of
/// them when they fit. Otherwise as many as leave room for the overflow button `overflow` wide
/// after them - from the first on while the active tab `active` is among those, else a run that
/// ends at the active tab and reaches as far back, then forward, as fits. The active tab is shown
/// even when it alone is wider than the room.
pub(super) fn visible_range(
    widths: &[f32],
    active: usize,
    available: f32,
    overflow: f32,
) -> Range<usize> {
    let row = |range: Range<usize>| {
        widths[range.clone()].iter().sum::<f32>() + SPACING * range.len().saturating_sub(1) as f32
    };
    if widths.is_empty() || row(0..widths.len()) <= available {
        return 0..widths.len();
    }
    let active = active.min(widths.len().saturating_sub(1));
    let room = available - overflow - SPACING;
    let first = (0..widths.len())
        .take_while(|&end| row(0..end + 1) <= room)
        .count();
    if active < first {
        return 0..first;
    }
    let mut start = active;
    while start > 0 && row(start - 1..active + 1) <= room {
        start -= 1;
    }
    let mut end = active + 1;
    while end < widths.len() && row(start..end + 1) <= room {
        end += 1;
    }
    start..end
}

#[cfg(test)]
#[path = "overflow_tests.rs"]
mod tests;
