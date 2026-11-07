//! The books of the dialog windows: which dialog has a window of its own under which id, where
//! each was last, and which windows to open or close so the open ones are the dialogs the app's
//! state shows; and those of the main window's menu bar (RC-080-18) and of the app's return
//! from another program (RC-100-69). No window is touched here.

use std::time::Instant;

use iced::window;
use rc_engine::JobId;

pub use super::dialog_window::DialogWindow;
use super::front::{self, HandBack, Up};
use crate::app::{
    palette::menu_bar::MenuBar,
    refocus::Refocus,
    window_state::{Geometry, WindowChange, WindowState},
};

/// Where an open dialog window is on its way to the screen (RC-080-23): made, or taken back
/// hidden, and not yet placed; being placed and shown - its main window may be locked already;
/// shown. Only a shown one closes: one whose dialog closed earlier goes when it is ready, never
/// owned (`App::on_dialog_window`), so no close can overtake the lock of its showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Opening,
    Showing,
    Shown,
}

/// An open dialog window and where it is.
#[derive(Debug, Clone, Copy)]
struct Open {
    kind: DialogWindow,
    id: window::Id,
    state: WindowState,
    stage: Stage,
    /// The window that had the keyboard when it opened: it takes it back when this one goes
    /// (RC-080-73).
    opener: Option<window::Id>,
}

#[derive(Debug, Default)]
pub struct DialogWindows {
    /// The main window; `None` only before it opens (and in the tests).
    main: Option<window::Id>,
    open: Vec<Open>,
    /// Where each dialog window was when it closed last, from the database or this run.
    stored: Vec<(DialogWindow, Geometry)>,
    /// The main window's menu bar: its setting and the look it was given.
    pub menu_bar: MenuBar,
    /// The setting `queue.window_own`, as read: the queue opens in a window of its own.
    pub queue_own: bool,
    /// The setting `appearance.dialog_windows`, as read: the dialogs open in windows of their
    /// own.
    pub own: bool,
    /// Whether a closed dialog's window can be hidden for its next opening
    /// ([`DialogWindow::reused`]): not on Wayland (`platform::dialog_window::hides`).
    pub hides: bool,
    /// The window that has the keyboard, as the system last said; `None` while unknown.
    focused: Option<window::Id>,
    /// Whether the app came back from another program, for the panels' refresh (RC-100-69).
    pub refocus: Refocus,
    /// The file operation's question the main window showed when last looked at.
    asked: Option<JobId>,
    /// The windows hidden since their dialog closed, for its next opening
    /// ([`DialogWindow::reused`]).
    parked: Vec<(DialogWindow, window::Id)>,
    /// The small question's window, once shown, and the window it is owned by and locks
    /// (`question_window.rs`).
    pub question_owner: Option<(window::Id, window::Id)>,
    /// The focus handed back to the window on top, not landed yet (RC-080-73).
    handing: HandBack,
}

impl DialogWindows {
    /// The books for the main window `main`, with the places the dialogs were left at.
    pub fn new(main: window::Id, stored: Vec<(DialogWindow, Geometry)>) -> Self {
        Self {
            main: Some(main),
            open: Vec::new(),
            stored,
            menu_bar: MenuBar::default(),
            queue_own: false,
            own: false,
            hides: crate::platform::dialog_window::hides(),
            focused: None,
            refocus: Refocus::default(),
            asked: None,
            parked: Vec::new(),
            question_owner: None,
            handing: HandBack::default(),
        }
    }

    /// The file operations ask `asking` now - a conflict or an error, drawn in the main window.
    /// The main window, to bring to the front, when the question is new and a modeless dialog's
    /// window has the keyboard: there the question would wait unseen and its keys would go to
    /// that dialog (RC-080-15). A modal one locks the main window; its question waits for it.
    pub fn question(&mut self, asking: Option<JobId>) -> Option<window::Id> {
        let new = self.new_question(asking);
        let away = self.focused_kind().is_some_and(|kind| !kind.modal());
        self.main.filter(|_| new && away)
    }

    /// Whether the file operations ask `asking` now and did not when last looked at.
    pub fn new_question(&mut self, asking: Option<JobId>) -> bool {
        let new = asking.is_some() && asking != self.asked;
        self.asked = asking;
        new
    }

    /// Window `id` took the keyboard (`true`) or let go of it.
    pub fn focus(&mut self, id: window::Id, focused: bool) {
        if focused {
            self.focused = Some(id);
        } else if self.focused == Some(id) {
            self.focused = None;
        }
    }

    /// Whether the window of `kind` is open and has the keyboard.
    pub fn has_focus(&self, kind: DialogWindow) -> bool {
        self.id_of(kind).is_some_and(|id| self.focused == Some(id))
    }

    /// The dialog whose window has the keyboard; `None` while the main window has it, or none.
    pub fn focused_kind(&self) -> Option<DialogWindow> {
        self.focused.and_then(|id| self.kind_of(id))
    }

    pub fn main(&self) -> Option<window::Id> {
        self.main
    }

    /// The dialog `id` is the window of; `None` for the main window and closed ones.
    pub fn kind_of(&self, id: window::Id) -> Option<DialogWindow> {
        self.open
            .iter()
            .find(|open| open.id == id)
            .map(|open| open.kind)
    }

    /// The window of `kind`, while it is open.
    pub fn id_of(&self, kind: DialogWindow) -> Option<window::Id> {
        self.open
            .iter()
            .find(|open| open.kind == kind)
            .map(|open| open.id)
    }

    /// The window of `kind`, while it is open and shown.
    pub fn visible(&self, kind: DialogWindow) -> Option<window::Id> {
        self.open
            .iter()
            .find(|open| open.kind == kind && open.stage == Stage::Shown)
            .map(|open| open.id)
    }

    /// The dialogs to open a window for and the windows to close, so the open windows are the
    /// `wanted` dialogs; a window on its way to the screen closes once it is shown.
    pub fn difference(&self, wanted: &[DialogWindow]) -> (Vec<DialogWindow>, Vec<window::Id>) {
        let opening = wanted
            .iter()
            .copied()
            .filter(|kind| self.id_of(*kind).is_none())
            .collect();
        let closing = self
            .open
            .iter()
            .filter(|open| open.stage == Stage::Shown && !wanted.contains(&open.kind))
            .map(|open| open.id)
            .collect();
        (opening, closing)
    }

    /// Where `kind` opens: where another Lister is while one is open - the Listers share one
    /// place, as Total Commander opens each at the last one's -, else where it was left, else at
    /// its first size centred over the main window `main` - over the screen while the main
    /// window's place is unknown or maximized.
    pub fn geometry(&self, kind: DialogWindow, main: Geometry) -> Geometry {
        let place = kind.key();
        if let Some(open) = self.open.iter().rev().find(|open| open.kind.key() == place) {
            return open.state.geometry();
        }
        if let Some((_, stored)) = self.stored.iter().find(|(stored, _)| stored.key() == place) {
            return *stored;
        }
        let size = kind.default_size();
        let position = main.position.filter(|_| !main.maximized).map(|(x, y)| {
            (
                x + ((main.width - size.width) / 2.0).max(0.0),
                y + ((main.height - size.height) / 2.0).max(0.0),
            )
        });
        Geometry {
            width: size.width,
            height: size.height,
            position,
            maximized: false,
        }
    }

    /// The window `id` of `kind` is open at `geometry` and shown at once: the tests' shortcut past
    /// the two steps of [`DialogWindows::opening`] and its `Shown`.
    #[cfg(test)]
    pub fn opened(&mut self, kind: DialogWindow, id: window::Id, geometry: Geometry) {
        self.book(kind, id, geometry, Stage::Shown);
    }

    /// The window `id` of `kind` is being made at `geometry`, invisible until it is placed.
    pub fn opening(&mut self, kind: DialogWindow, id: window::Id, geometry: Geometry) {
        self.book(kind, id, geometry, Stage::Opening);
    }

    fn book(&mut self, kind: DialogWindow, id: window::Id, geometry: Geometry, stage: Stage) {
        self.open.push(Open {
            kind,
            id,
            state: WindowState::new(Some(geometry)),
            stage,
            opener: self.focused.filter(|focused| *focused != id),
        });
    }

    /// The window of `kind` hidden since its dialog closed opens again at `geometry`, where it
    /// was hidden: its id, `None` when there is none to take back.
    pub fn reopen(&mut self, kind: DialogWindow, geometry: Geometry) -> Option<window::Id> {
        let at = self.parked.iter().position(|(parked, _)| *parked == kind)?;
        let (_, id) = self.parked.remove(at);
        self.opening(kind, id, geometry);
        Some(id)
    }

    /// The window `id` of `kind`, closed in the books, stays hidden for its dialog's next
    /// opening.
    pub fn park(&mut self, kind: DialogWindow, id: window::Id) {
        self.parked.push((kind, id));
    }

    /// The window `id` is being placed and shown (`true`) or is shown.
    pub fn advance(&mut self, id: window::Id, showing: bool) {
        if let Some(open) = self.open.iter_mut().find(|open| open.id == id) {
            open.stage = if showing {
                Stage::Showing
            } else {
                Stage::Shown
            };
        }
    }

    /// The window `id` closes: its dialog and where it was, kept for the next opening; `None`
    /// for a window that is no dialog's.
    pub fn closed(&mut self, id: window::Id) -> Option<(DialogWindow, Geometry)> {
        let at = self.open.iter().position(|open| open.id == id)?;
        let open = self.open.remove(at);
        let geometry = open.state.geometry();
        self.stored
            .retain(|(kind, _)| kind.key() != open.kind.key());
        self.stored.push((open.kind, geometry));
        Some((open.kind, geometry))
    }

    /// The window `id` moved or changed its size; `false` when it is no dialog's.
    pub fn observe(&mut self, id: window::Id, change: WindowChange, maximized: bool) -> bool {
        match self.open.iter_mut().find(|open| open.id == id) {
            Some(open) => {
                open.state.observe(change, maximized);
                true
            }
            None => false,
        }
    }

    /// The modal dialog's window opened last and on its way to the screen or shown - no
    /// question's: in front of the other ones, so a question's window is owned by it
    /// (`question_window.rs`).
    pub fn front_modal(&self) -> Option<window::Id> {
        self.open
            .iter()
            .rev()
            .find(|open| {
                open.kind.modal() && !open.kind.is_question() && open.stage != Stage::Opening
            })
            .map(|open| open.id)
    }

    /// Whether a modal dialog's window is open: the main window stays locked while one is. One
    /// not yet being shown has locked nothing yet and locks it itself when it shows.
    pub fn modal_open(&self) -> bool {
        self.open
            .iter()
            .any(|open| open.kind.modal() && open.stage != Stage::Opening)
    }

    /// The dialogs' windows on the screen or being shown - not those still being made -, in the
    /// order they opened.
    pub(super) fn up(&self) -> Vec<Up> {
        self.open
            .iter()
            .filter(|open| open.stage != Stage::Opening)
            .map(|open| Up {
                kind: open.kind,
                id: open.id,
            })
            .collect()
    }

    /// The window to give the keyboard once window `id` closes or hides (`front::after_close`),
    /// asked before it is booked closed; `None` while the keyboard is in a window that stays or
    /// in another program - a window that goes without the keyboard takes nobody's focus.
    pub fn front_after(&self, id: window::Id) -> Option<window::Id> {
        // The keyboard in `id`, or in a window that went in the same round (a question closed
        // before its owner) and told no loss yet.
        let ours = self.focused.is_some_and(|focused| {
            focused == id || (Some(focused) != self.main && self.kind_of(focused).is_none())
        });
        if !ours {
            return None;
        }
        let opener = self
            .open
            .iter()
            .find(|open| open.id == id)
            .and_then(|open| open.opener);
        let up: Vec<Up> = self.up().into_iter().filter(|up| up.id != id).collect();
        front::after_close(&up, opener, self.main)
    }

    /// Window `id` took the keyboard at `now`: the window on top to hand it back to while `id`
    /// lies under it (`front::under`), once per hand-back under way.
    pub fn hand_back(&mut self, id: window::Id, now: Instant) -> Option<window::Id> {
        let up = self.up();
        let owner = self
            .question_owner
            .filter(|(question, _)| up.iter().any(|up| up.id == *question))
            .map(|(_, owner)| owner);
        let target = front::under(&up, owner, self.main, id);
        self.handing.hand(id, target, now)
    }

    /// Where every open dialog window is: stored with the main window's place when the app ends.
    pub fn open_geometries(&self) -> Vec<(DialogWindow, Geometry)> {
        self.open
            .iter()
            .map(|open| (open.kind, open.state.geometry()))
            .collect()
    }
}

#[cfg(test)]
#[path = "book_tests.rs"]
mod tests;
