//! Which window has the keyboard when a dialog's window goes, and where a focus that lands under
//! a question or a modal dialog goes back to (RC-080-73).
//!
//! A window that closes or hides while it has the keyboard leaves the choice of the next one to
//! the system: Windows takes the next window in the z-order, an X11 window manager or a Wayland
//! compositor whichever it likes - often another program's, and rCommander "goes to the
//! background". So the app names it ([`after_close`]): the open question, else the modal dialog
//! opened last, else the window the closing one was opened from, else the main window
//! (`platform::dialog_window::close`, `hide`).
//!
//! On Windows a modal dialog's or a question's owner is disabled and the system keeps the focus
//! off it. Linux and macOS know no such lock: a click on the main window raised it over the modal
//! dialog, which "vanished behind it". There a focus that lands on a window under the top one
//! ([`under`]) is handed back to it (`refocus.rs`), once per [`SETTLE`] while it has not landed
//! ([`HandBack`]): a compositor that refuses the focus (Wayland's focus-stealing prevention, no
//! error, no effect) or a window manager that gives it back can never make the two windows play
//! ping-pong.

use std::time::{Duration, Instant};

use iced::window;

use super::DialogWindow;

/// How long a hand-back that has not landed keeps another one to the same window from being
/// sent.
pub const SETTLE: Duration = Duration::from_millis(500);

/// A dialog's window on the screen or being shown, in the order the windows opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Up {
    pub kind: DialogWindow,
    pub id: window::Id,
}

/// The window on top of the others that keeps the keyboard: the question's - one at a time -,
/// else the modal dialog's opened last; `None` while neither is up.
#[must_use]
pub fn top(up: &[Up]) -> Option<window::Id> {
    up.iter()
        .rev()
        .find(|window| window.kind.is_question())
        .or_else(|| up.iter().rev().find(|window| window.kind.modal()))
        .map(|window| window.id)
}

/// The window that takes the keyboard once a dialog's window went that had it: the [`top`] one
/// of those still `up`, else the window it was opened from (`opener`) while that is the main
/// window or still up, else the main window.
#[must_use]
pub fn after_close(
    up: &[Up],
    opener: Option<window::Id>,
    main: Option<window::Id>,
) -> Option<window::Id> {
    let opener = opener.filter(|id| Some(*id) == main || up.iter().any(|window| window.id == *id));
    top(up).or(opener).or(main)
}

/// The window a focus on `id` is handed back to: the [`top`] one, when `id` lies under it - the
/// main window while a modal dialog or a modal question is up, a modal dialog's window under a
/// newer one or a question, the window that owns the question (`question_owner`). A modeless
/// dialog's window works on beside them, as on Windows. `None` when the focus stays.
#[must_use]
pub fn under(
    up: &[Up],
    question_owner: Option<window::Id>,
    main: Option<window::Id>,
    id: window::Id,
) -> Option<window::Id> {
    let top = top(up)?;
    if id == top {
        return None;
    }
    let below = match up.iter().find(|window| window.id == id) {
        Some(window) => {
            !window.kind.is_question() && (window.kind.modal() || Some(id) == question_owner)
        }
        None => Some(id) == main && up.iter().any(|window| window.kind.modal()),
    };
    below.then_some(top)
}

/// The hand-back sent last and not landed yet: the window and when.
#[derive(Debug, Default)]
pub struct HandBack {
    pending: Option<(window::Id, Instant)>,
}

impl HandBack {
    /// The focus landed on `id` at `now`, which [`under`] hands back to `target`: the window to
    /// give the keyboard, `None` while the same hand-back is under way for less than [`SETTLE`].
    /// A focus on the pending window means it landed.
    pub fn hand(
        &mut self,
        id: window::Id,
        target: Option<window::Id>,
        now: Instant,
    ) -> Option<window::Id> {
        if self.pending.is_some_and(|(to, _)| to == id) {
            self.pending = None;
        }
        let target = target?;
        if let Some((to, at)) = self.pending
            && to == target
            && now.saturating_duration_since(at) < SETTLE
        {
            return None;
        }
        self.pending = Some((target, now));
        Some(target)
    }
}

#[cfg(test)]
#[path = "front_tests.rs"]
mod tests;
