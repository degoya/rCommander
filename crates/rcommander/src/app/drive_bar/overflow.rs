//! A bar that does not fit its width (`docs/design.md`, Bars): as many buttons as fit stay, in
//! their order, and the rest goes behind an overflow button `»` at the end; nothing is clipped
//! unseen. The widths are estimated from the font (`theme::ellipsis::estimate_width`), as the
//! view holds no renderer. `»` opens the drive menu with exactly the items it holds
//! (`menu::Scope::Overflow`), which [`bar_items`] names for the view and the menu alike.

use rc_engine::{Drive, DriveGroup};

use crate::theme::ellipsis::estimate_width;

/// The space between two buttons of a bar.
pub(super) const SPACING: f32 = 3.0;
/// A button's padding on each side, plus its border.
const PADDING: f32 = 7.0 + 1.0;
/// The space between a button's parts (symbol, name, arrow).
const GAP: f32 = 4.0;

/// One item of a drive bar, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarItem<'a> {
    /// The space between two groups (drives, WSL, servers).
    Gap,
    /// A folder of the connection tree the bar shows (RC-100-63), by its path from the root: one
    /// button for every server inside it.
    Folder(&'a [String]),
    /// The entry at this index of the drives.
    Drive(usize),
}

/// The bar's items for `drives`: every drive, WSL distribution and marked server a button of
/// its own, a marked folder (by default each top-level one, `Drive::folder_buttons`) one button
/// where its first server stands, a gap between groups. A folder without servers has no entry,
/// so no button.
pub(super) fn bar_items(drives: &[Drive]) -> Vec<BarItem<'_>> {
    let mut items = Vec::with_capacity(drives.len() + 2);
    let mut group = None;
    // The folders listed so far: each is one button, whatever it holds.
    let mut folders: Vec<&[String]> = Vec::new();
    for (index, drive) in drives.iter().enumerate() {
        // A server is a button of its own when marked; its folder's button reaches it anyway.
        let own = drive.group != DriveGroup::Servers || drive.favourite;
        let new: Vec<&[String]> = drive
            .folder_buttons
            .iter()
            .filter_map(|&length| drive.folder.get(..length))
            .filter(|path| !path.is_empty() && !folders.contains(path))
            .collect();
        if !own && new.is_empty() {
            continue;
        }
        if group.is_some_and(|group| group != drive.group) {
            items.push(BarItem::Gap);
        }
        group = Some(drive.group);
        for path in new {
            folders.push(path);
            items.push(BarItem::Folder(path));
        }
        if own {
            items.push(BarItem::Drive(index));
        }
    }
    items
}

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
