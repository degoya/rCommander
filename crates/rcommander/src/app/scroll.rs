//! A keyboard cursor kept in view in a scrolled list: the drive menu, the folder history and
//! hotlist popups, the command line's history.
//!
//! Each such list sits in a `scrollable` with an id of its own. After every move of its cursor,
//! and when it opens, the list is snapped so the cursor's line stands where its share of the list
//! says: the first line at the top, the last at the bottom, one in between proportionally - which
//! keeps it inside the visible part whatever the lines' heights.

use iced::widget::{
    Id,
    operation::{self, RelativeOffset},
};

use super::Effect;

/// The share of the scroll range that shows line `line` of `lines`: 0 for the first, 1 for the
/// last.
#[must_use]
pub fn share(line: usize, lines: usize) -> f32 {
    if lines <= 1 {
        return 0.0;
    }
    (line.min(lines - 1) as f32 / (lines - 1) as f32).clamp(0.0, 1.0)
}

/// Snaps the scrollable `id` so line `line` of `lines` is in view.
#[must_use]
pub fn keep_in_view(id: &'static str, line: usize, lines: usize) -> Effect {
    let offset = RelativeOffset {
        x: None,
        y: Some(share(line, lines)),
    };
    Effect::Task(operation::snap_to(Id::new(id), offset))
}

#[cfg(test)]
#[path = "scroll_tests.rs"]
mod tests;
