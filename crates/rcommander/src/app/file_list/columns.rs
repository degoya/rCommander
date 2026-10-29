//! The full view's columns (`docs/design.md`, Tables): which there are, their widths - the fixed
//! ones keep theirs, the name takes the rest - and the key a click on each title sorts by. The
//! rows and the header both take them from here, so a title's click area is its whole column.
//!
//! The fixed widths are the person's (RC-010-20, `docs/design.md`, Resizing): dragged at a
//! title's left edge, shared by both panels as in Total Commander, kept as settings at the
//! default list font size and scaled with the chosen one. In a panel too narrow for them they
//! give way, down to their least widths, so the name keeps [`NAME_MIN`].
//!
//! The owner column (RC-060-02) is the person's to switch on (`columns.owner`, off by default),
//! and shows where the backend names owners: on a server over SFTP, local folders on Unix.

use rc_engine::{
    SortKey,
    settings::{
        DATE_WIDTH_KEY, EXT_WIDTH_KEY, OWNER_WIDTH_KEY, PACKED_WIDTH_KEY, RIGHTS_WIDTH_KEY,
        SIZE_WIDTH_KEY, SettingKind, setting,
    },
};

use crate::app::icons::ICON_SIZE;

pub(super) const SCROLLBAR_WIDTH: f32 = 6.0;
/// The space between two columns of a row; in the header it is the grip that drags the width.
pub(super) const SPACING: f32 = 8.0;
/// What a row takes besides its columns: the list's padding (4 on each side), the row's (6 on
/// each side), and the cursor's pill (3) with the space after it.
const ROW_CHROME: f32 = 4.0 * 2.0 + 6.0 * 2.0 + 3.0 + SPACING;
/// The space between a row's icon and its name.
pub(super) const ICON_GAP: f32 = 6.0;
/// The name's least width, icon and text, at the default font size, while the fixed columns can
/// still give way.
pub(super) const NAME_MIN: f32 = 80.0;

/// A column of the full view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Name,
    Ext,
    Size,
    /// The packed size, inside an archive.
    Packed,
    Date,
    /// The rights, on a server.
    Rights,
    /// User and group, where the backend names them and the person switched it on.
    Owner,
}

impl Column {
    /// The columns whose width is the person's, in the order of [`ColumnWidths`].
    pub(in crate::app) const FIXED: [Self; 6] = [
        Self::Ext,
        Self::Size,
        Self::Packed,
        Self::Date,
        Self::Rights,
        Self::Owner,
    ];

    /// The key a click on the column's title sorts by; the packed size, the rights and the owner
    /// sort by none.
    pub(super) fn sort_key(self) -> Option<SortKey> {
        match self {
            Self::Name => Some(SortKey::Name),
            Self::Ext => Some(SortKey::Ext),
            Self::Size => Some(SortKey::Size),
            Self::Date => Some(SortKey::Date),
            Self::Packed | Self::Rights | Self::Owner => None,
        }
    }

    /// Whether the column's values stand at its right edge: the sizes.
    pub(super) fn right(self) -> bool {
        matches!(self, Self::Size | Self::Packed)
    }

    /// The setting that keeps the column's width; `None` for the name, which takes the rest.
    pub(in crate::app) fn setting(self) -> Option<&'static str> {
        match self {
            Self::Name => None,
            Self::Ext => Some(EXT_WIDTH_KEY),
            Self::Size => Some(SIZE_WIDTH_KEY),
            Self::Packed => Some(PACKED_WIDTH_KEY),
            Self::Date => Some(DATE_WIDTH_KEY),
            Self::Rights => Some(RIGHTS_WIDTH_KEY),
            Self::Owner => Some(OWNER_WIDTH_KEY),
        }
    }

    /// The least, the largest and the default width at the default font size, from the
    /// column's schema row; nothing for the name.
    pub(in crate::app) fn bounds(self) -> (f32, f32, f32) {
        match self.setting().and_then(setting).map(|spec| spec.kind) {
            Some(SettingKind::Int { min, max, default }) => {
                (min as f32, max as f32, default as f32)
            }
            _ => (0.0, 0.0, 0.0),
        }
    }

    fn slot(self) -> Option<usize> {
        Self::FIXED.iter().position(|fixed| *fixed == self)
    }

    /// The column's width at `scale` (the list font's, `AreaFont::column_scale`) in `widths`;
    /// `None` for the name, which takes what the others leave.
    pub(super) fn width(self, widths: &ColumnWidths, scale: f32) -> Option<f32> {
        widths.get(self).map(|width| width * scale)
    }
}

/// The fixed columns' widths at the default font size, shared by both panels (RC-010-20), and
/// whether the owner column is switched on (RC-060-02).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnWidths {
    widths: [f32; 6],
    /// The setting `columns.owner`; off until it is read.
    pub(in crate::app) owner: bool,
}

impl Default for ColumnWidths {
    fn default() -> Self {
        Self {
            widths: Column::FIXED.map(|column| column.bounds().2),
            owner: false,
        }
    }
}

impl ColumnWidths {
    /// The width of `column`; `None` for the name.
    pub(in crate::app) fn get(&self, column: Column) -> Option<f32> {
        column.slot().map(|slot| self.widths[slot])
    }

    /// Sets the width of `column`, held within its row's bounds; the name has none to set.
    pub(in crate::app) fn set(&mut self, column: Column, width: f32) {
        let (min, max, _) = column.bounds();
        if let Some(slot) = column.slot()
            && width.is_finite()
        {
            self.widths[slot] = width.clamp(min, max);
        }
    }
}

/// The columns from left to right: the packed size inside an archive, the rights on a server,
/// the owner where it is switched on and known.
pub(super) fn columns(archive: bool, rights: bool, owner: bool) -> Vec<Column> {
    let mut columns = vec![Column::Name, Column::Ext, Column::Size];
    if archive {
        columns.push(Column::Packed);
    }
    columns.push(Column::Date);
    if rights {
        columns.push(Column::Rights);
    }
    if owner {
        columns.push(Column::Owner);
    }
    columns
}

/// The width of each of `columns` in a list `width` wide (its scrollbar included) at `scale`:
/// each fixed column its own from `widths`, the name's cell (icon and text) what they leave.
/// Where that would be less than [`NAME_MIN`], the fixed columns give way, each in proportion to
/// what it has above its least width; in a list not yet measured, or too narrow even then, the
/// name gets nothing.
pub(super) fn split(width: f32, columns: &[Column], widths: &ColumnWidths, scale: f32) -> Vec<f32> {
    let mut sizes: Vec<f32> = columns
        .iter()
        .map(|column| column.width(widths, scale).unwrap_or(0.0))
        .collect();
    let gaps = SPACING * columns.len().saturating_sub(1) as f32;
    let room = width - SCROLLBAR_WIDTH - ROW_CHROME - gaps;
    let short = sizes.iter().sum::<f32>() + NAME_MIN * scale - room;
    if width > 0.0 && short > 0.0 {
        let slack: Vec<f32> = columns
            .iter()
            .zip(&sizes)
            .map(|(column, size)| (size - column.bounds().0 * scale).max(0.0))
            .collect();
        let total: f32 = slack.iter().sum();
        if total > 0.0 {
            let share = short.min(total) / total;
            for (size, slack) in sizes.iter_mut().zip(&slack) {
                *size -= slack * share;
            }
        }
    }
    let name = (room - sizes.iter().sum::<f32>()).max(0.0);
    for (size, column) in sizes.iter_mut().zip(columns) {
        if *column == Column::Name {
            *size = name;
        }
    }
    sizes
}

/// The name's own width in its cell `cell` wide, after the icon when `icon`.
pub(super) fn name_room(cell: f32, icon: bool) -> f32 {
    let icon = if icon { ICON_SIZE + ICON_GAP } else { 0.0 };
    (cell - icon).max(0.0)
}

/// The name's width in one column of the brief view, `columns` of them in a list `width` wide.
pub(super) fn brief_width(width: f32, columns: usize, icon: bool) -> f32 {
    let columns = columns.max(1) as f32;
    // The list's padding and the 4 between the brief columns.
    let inner = width - SCROLLBAR_WIDTH - 4.0 * 2.0 - 4.0 * (columns - 1.0);
    let icon = if icon { ICON_SIZE + ICON_GAP } else { 0.0 };
    // The row's padding, the pill and the 6 after it.
    (inner / columns - 6.0 * 2.0 - 3.0 - 6.0 - icon).max(0.0)
}

#[cfg(test)]
#[path = "columns_tests.rs"]
mod tests;
