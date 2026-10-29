//! A bar that does not fit its width (`docs/design.md`, Bars): as many buttons as fit stay, in
//! their order, and the rest goes behind an overflow button `»` at the end; nothing is clipped
//! unseen. The widths are estimated from the font (`theme::ellipsis::estimate_width`), as the
//! view holds no renderer.

use crate::theme::ellipsis::estimate_width;

/// The space between two buttons of a bar.
pub(super) const SPACING: f32 = 3.0;
/// A button's padding on each side, plus its border.
const PADDING: f32 = 7.0 + 1.0;
/// The space between a button's parts (symbol, name, arrow).
const GAP: f32 = 4.0;

/// The width of a bar button showing `parts` side by side at `size`.
pub(super) fn button_width(parts: &[&str], size: f32) -> f32 {
    let text: f32 = parts.iter().map(|part| estimate_width(part, size)).sum();
    text + GAP * parts.len().saturating_sub(1) as f32 + 2.0 * PADDING
}

/// How many of the items `widths` wide fit in `available` pixels, [`SPACING`] apart, from the
/// first on. All of them when they fit; otherwise as many as leave room for the overflow button
/// `overflow` wide after them.
pub(super) fn visible_count(widths: &[f32], available: f32, overflow: f32) -> usize {
    let row =
        |items: &[f32]| items.iter().sum::<f32>() + SPACING * items.len().saturating_sub(1) as f32;
    if row(widths) <= available {
        return widths.len();
    }
    let room = available - overflow - SPACING;
    (0..widths.len())
        .take_while(|&count| row(&widths[..=count]) <= room)
        .count()
}

#[cfg(test)]
#[path = "overflow_tests.rs"]
mod tests;
