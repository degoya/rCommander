//! The panels' chrome (`docs/design.md`): striped list and table rows, the folder tabs, the cells
//! of a column header, the drive line.

use std::sync::atomic::{AtomicBool, Ordering};

use iced::{
    Background, Border,
    border::Radius,
    widget::{button, container},
};

use super::{Tokens, row};

/// The setting `appearance.alternate_rows`, read at the start and on every change; on until then.
static STRIPES: AtomicBool = AtomicBool::new(true);

/// Switches the alternate colour of every other row in the lists and tables.
pub fn set_stripes(on: bool) {
    STRIPES.store(on, Ordering::Relaxed);
}

/// Whether row `index` of a list (`..` is row 0) is drawn on the alternate colour: the stripe
/// stays with the entry, not with the screen.
pub fn alternate(index: usize) -> bool {
    alternate_when(STRIPES.load(Ordering::Relaxed), index)
}

/// Every odd row, while the stripes are `on`.
fn alternate_when(on: bool, index: usize) -> bool {
    on && index % 2 == 1
}

/// [`row`] in a striped list: an `alternate` row without cursor or selection on `stripe_fill`.
pub fn striped_row(
    tokens: &Tokens,
    cursor: bool,
    active: bool,
    selected: bool,
    alternate: bool,
) -> container::Style {
    let mut style = row(tokens, cursor, active, selected);
    if alternate && style.background.is_none() {
        style.background = Some(Background::Color(tokens.stripe_fill));
    }
    style
}

/// A folder tab: the active one in the list's colour with rounded top corners, as part of the
/// list under it; the others on the bar's colour, dimmed, filled while the pointer is on them.
pub fn tab(tokens: &Tokens, active: bool, status: button::Status) -> button::Style {
    let fill = match (active, status) {
        (true, _) => Some(tokens.surface),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            Some(tokens.cursor_fill_inactive)
        }
        _ => None,
    };
    button::Style {
        background: fill.map(Background::Color),
        text_color: if active {
            tokens.text
        } else {
            tokens.text_secondary
        },
        border: Border::default().rounded(Radius {
            top_left: 6.0,
            top_right: 6.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }),
        ..button::Style::default()
    }
}

/// A cell of a column header, the whole of it a button: filled while the pointer is on it.
pub fn header_cell(tokens: &Tokens, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then_some(Background::Color(tokens.cursor_fill_inactive)),
        text_color: tokens.text_secondary,
        border: Border::default().rounded(4.0),
        ..button::Style::default()
    }
}

/// A panel's drive line (RC-010-22): set off from the window in the header's tone, the active
/// panel's in the accent's tint of its cursor row; rounded at the top, its hairline under it.
pub fn drive_line(tokens: &Tokens, active: bool) -> container::Style {
    container::Style {
        background: Some(Background::Color(if active {
            tokens.cursor_fill
        } else {
            tokens.header
        })),
        border: Border::default().rounded(Radius {
            top_left: 4.0,
            top_right: 4.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }),
        text_color: Some(tokens.text),
        ..container::Style::default()
    }
}

/// The hairline under a drive line: the accent under the active panel's, the stroke elsewhere.
pub fn drive_line_rule(tokens: &Tokens, active: bool) -> container::Style {
    container::Style {
        background: Some(Background::Color(if active {
            tokens.accent
        } else {
            tokens.stroke
        })),
        ..container::Style::default()
    }
}

#[cfg(test)]
#[path = "chrome_tests.rs"]
mod tests;
