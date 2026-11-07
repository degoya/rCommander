//! A dialog in a window of its own (RC-080-15): shown after it is placed, closed or hidden after
//! the main window takes input again, and the window to the front given the keyboard it had.
//!
//! The window opens invisible. On Windows the main window becomes its owner - it stays in front
//! of it and minimizes with it - it gets its own Alt+Tab and taskbar entry, it is moved onto the
//! monitors' work areas ([`clamp`]) and, for a modal one, the main window takes no input while it
//! is open (`windows/dialog_window.rs`); a modeless one - the queue, RC-010-14 - leaves the main
//! window working. Under X11 it is marked as a dialog transient for its owner, so the window
//! manager keeps it in front of it (`x11_dialog.rs`, RC-080-73); Wayland and macOS have no such
//! hint from outside winit. Then it shows and takes the keyboard.
//!
//! When it closes or hides, the window the app names takes the keyboard (`front`, chosen by
//! `app/dialog_windows/front.rs`) on every system: left to itself, the system gives it to
//! whichever window it likes, often another program's (RC-080-73).

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

/// Shows the dialog window `id`, opened invisible - under X11 marked as the dialog of `owner`
/// first.
#[cfg(not(windows))]
pub fn show<T: Send + 'static>(id: window::Id, owner: Option<window::Id>, _modal: bool) -> Task<T> {
    transient(id, owner).chain(shown(id))
}

/// Marks the dialog window `id`, not yet shown, as the dialog of `owner` under X11; nothing
/// without an owner or on Wayland. Off the event loop's thread, and done before the window maps,
/// when the window manager reads the hints.
#[cfg(all(unix, not(target_os = "macos")))]
fn transient<T: Send + 'static>(id: window::Id, owner: Option<window::Id>) -> Task<T> {
    use crate::recover::LogPanic as _;
    let Some(owner) = owner.filter(|_| !wayland(|name| std::env::var_os(name))) else {
        return Task::none();
    };
    window::run(owner, super::x11_dialog::x_window_of)
        .then(move |owner| {
            window::run(id, super::x11_dialog::x_window_of).map(move |dialog| (dialog, owner))
        })
        .then(|windows| match windows {
            (Some(dialog), Some(owner)) => Task::future(async move {
                let marked = tokio::task::spawn_blocking(move || {
                    super::x11_dialog::mark_dialog(dialog, owner)
                })
                .await
                .log_panic("dialog_window.x11_hints");
                if let Ok(Err(error)) = marked {
                    // Only the window manager's stacking is missing then.
                    tracing::debug!(%error, "a dialog window's X11 hints were not set");
                }
            })
            .discard(),
            // Not an X11 window (XWayland's absence, another back end).
            _ => Task::none(),
        })
}

/// macOS (and any other system without X11): no hint from outside winit.
#[cfg(all(not(windows), not(all(unix, not(target_os = "macos")))))]
fn transient<T: Send + 'static>(_id: window::Id, _owner: Option<window::Id>) -> Task<T> {
    Task::none()
}

fn shown<T: Send + 'static>(id: window::Id) -> Task<T> {
    window::set_mode(id, Mode::Windowed).chain(window::gain_focus(id))
}

/// Closes the dialog window `id` - a `modal` one after the main window `owner` takes input
/// again, so Windows can hand the focus back to it. A modeless one never locked the main window,
/// and unlocking it would free it from a modal one still open. The taskbar's relaunch comes off
/// it first (RC-100-67); `front` takes the keyboard last.
#[cfg(windows)]
pub fn close<T: Send + 'static>(
    id: window::Id,
    owner: Option<window::Id>,
    modal: bool,
    front: Option<window::Id>,
) -> Task<T> {
    super::taskbar::leave(id)
        .chain(released(owner, modal))
        .chain(window::close(id))
        .chain(focused(front))
}

/// Hides the dialog window `id` for its dialog's next opening (RC-080-23), after the main window
/// `owner` takes input again as [`close`] does; `front` takes the keyboard last.
#[cfg(windows)]
pub fn hide<T: Send + 'static>(
    id: window::Id,
    owner: Option<window::Id>,
    modal: bool,
    front: Option<window::Id>,
) -> Task<T> {
    released(owner, modal)
        .chain(window::set_mode(id, Mode::Hidden))
        .chain(focused(front))
}

/// The main window `owner` takes input again, when a `modal` dialog's window goes.
#[cfg(windows)]
fn released<T: Send + 'static>(owner: Option<window::Id>, modal: bool) -> Task<T> {
    match owner.filter(|_| modal) {
        Some(owner) => window::run(owner, super::windows::dialog_window::release).discard(),
        None => Task::none(),
    }
}

/// Closes the dialog window `id`; `front` takes the keyboard.
#[cfg(not(windows))]
pub fn close<T: Send + 'static>(
    id: window::Id,
    _owner: Option<window::Id>,
    _modal: bool,
    front: Option<window::Id>,
) -> Task<T> {
    window::close(id).chain(focused(front))
}

/// Hides the dialog window `id` for its dialog's next opening; `front` takes the keyboard.
#[cfg(not(windows))]
pub fn hide<T: Send + 'static>(
    id: window::Id,
    _owner: Option<window::Id>,
    _modal: bool,
    front: Option<window::Id>,
) -> Task<T> {
    window::set_mode(id, Mode::Hidden).chain(focused(front))
}

/// The window `front` takes the keyboard; nothing without one.
fn focused<T: Send + 'static>(front: Option<window::Id>) -> Task<T> {
    front.map_or_else(Task::none, window::gain_focus)
}

/// Whether a dialog's window can be hidden for its next opening ([`hide`]). Not on Wayland:
/// winit ignores a window's visibility there, so a hidden window stayed on screen, empty; it is
/// closed instead.
#[must_use]
pub fn hides() -> bool {
    !cfg!(all(unix, not(target_os = "macos"))) || !wayland(|name| std::env::var_os(name))
}

/// Whether winit takes Wayland, read as it does: `WAYLAND_DISPLAY` or `WAYLAND_SOCKET` set and
/// not empty, through `var`.
fn wayland(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    ["WAYLAND_DISPLAY", "WAYLAND_SOCKET"]
        .into_iter()
        .any(|name| var(name).is_some_and(|value| !value.is_empty()))
}

#[cfg(test)]
#[path = "dialog_window_tests.rs"]
mod tests;
