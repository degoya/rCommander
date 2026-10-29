//! A dialog in a window of its own (RC-080-15): shown after it is placed, closed or hidden after
//! the main window takes input again.
//!
//! The window opens invisible. On Windows the main window becomes its owner - it stays in front
//! of it and minimizes with it - it gets its own Alt+Tab and taskbar entry, it is moved onto the
//! monitors' work areas ([`clamp`]) and, for a modal one, the main window takes no input while it
//! is open (`windows/dialog_window.rs`); a modeless one - the queue, RC-010-14 - leaves the main
//! window working. Then it shows and takes the keyboard. Elsewhere it only shows.

use iced::{
    Task,
    window::{self, Mode},
};

/// A window's or a work area's rectangle in physical pixels, as Win32 reports them.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The height of the band at a window's top that must lie on a monitor: the title bar it is
/// moved by.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
const TITLE_BAND: i32 = 32;
/// How much of that band, across, keeps a window where it is.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
const MIN_VISIBLE: i32 = 96;

#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
impl Rect {
    /// The overlap of two rectangles; `None` when they do not meet.
    fn overlap(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > left && bottom > top).then(|| Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }

    fn area(self) -> i64 {
        i64::from(self.width) * i64::from(self.height)
    }
}

/// `window` where it can be grabbed: unchanged while enough of its title band lies on one of
/// the `areas` (the monitors' work areas, the primary first); else moved onto the area it
/// overlaps most, or onto the first when it overlaps none, no larger than that area and inside
/// it. Without areas it stays.
#[must_use]
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
pub fn clamp(window: Rect, areas: &[Rect]) -> Rect {
    let band = Rect {
        height: TITLE_BAND.min(window.height),
        ..window
    };
    let grabbable = areas.iter().any(|area| {
        band.overlap(*area)
            .is_some_and(|seen| seen.width >= MIN_VISIBLE.min(window.width))
    });
    if grabbable {
        return window;
    }
    let target = areas
        .iter()
        .copied()
        .max_by_key(|area| window.overlap(*area).map_or(0, Rect::area))
        .filter(|area| window.overlap(*area).is_some())
        .or_else(|| areas.first().copied());
    let Some(area) = target else {
        return window;
    };
    let width = window.width.min(area.width);
    let height = window.height.min(area.height);
    Rect {
        x: window.x.clamp(area.x, area.x + area.width - width),
        y: window.y.clamp(area.y, area.y + area.height - height),
        width,
        height,
    }
}

/// Shows the dialog window `id`, opened invisible, over the main window `owner`, which takes no
/// input while it is open when `modal`.
#[cfg(windows)]
pub fn show<T: Send + 'static>(id: window::Id, owner: Option<window::Id>, modal: bool) -> Task<T> {
    let owner = match owner {
        Some(owner) => crate::native_window::hwnd(owner),
        None => Task::done(None),
    };
    owner
        .then(move |owner| {
            window::run(id, move |dialog| {
                super::windows::dialog_window::own(dialog, owner, modal);
            })
        })
        .discard()
        .chain(shown(id))
}

/// Shows the dialog window `id`, opened invisible.
#[cfg(not(windows))]
pub fn show<T: Send + 'static>(
    id: window::Id,
    _owner: Option<window::Id>,
    _modal: bool,
) -> Task<T> {
    shown(id)
}

fn shown<T: Send + 'static>(id: window::Id) -> Task<T> {
    window::set_mode(id, Mode::Windowed).chain(window::gain_focus(id))
}

/// Closes the dialog window `id` - a `modal` one after the main window `owner` takes input
/// again, so Windows hands the focus back to it and not to another program. A modeless one never
/// locked the main window, and unlocking it would free it from a modal one still open.
#[cfg(windows)]
pub fn close<T: Send + 'static>(id: window::Id, owner: Option<window::Id>, modal: bool) -> Task<T> {
    released(owner, modal).chain(window::close(id))
}

/// Hides the dialog window `id` for its dialog's next opening (RC-080-23), after the main window
/// `owner` takes input again as [`close`] does.
#[cfg(windows)]
pub fn hide<T: Send + 'static>(id: window::Id, owner: Option<window::Id>, modal: bool) -> Task<T> {
    released(owner, modal).chain(window::set_mode(id, Mode::Hidden))
}

/// The main window `owner` takes input again, when a `modal` dialog's window goes.
#[cfg(windows)]
fn released<T: Send + 'static>(owner: Option<window::Id>, modal: bool) -> Task<T> {
    match owner.filter(|_| modal) {
        Some(owner) => window::run(owner, super::windows::dialog_window::release).discard(),
        None => Task::none(),
    }
}

/// Closes the dialog window `id`.
#[cfg(not(windows))]
pub fn close<T: Send + 'static>(
    id: window::Id,
    _owner: Option<window::Id>,
    _modal: bool,
) -> Task<T> {
    window::close(id)
}

/// Hides the dialog window `id` for its dialog's next opening.
#[cfg(not(windows))]
pub fn hide<T: Send + 'static>(
    id: window::Id,
    _owner: Option<window::Id>,
    _modal: bool,
) -> Task<T> {
    window::set_mode(id, Mode::Hidden)
}

#[cfg(test)]
#[path = "dialog_window_tests.rs"]
mod tests;
