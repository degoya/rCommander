//! Lister in a window of its own (RC-080-15): one window per open Lister, by its number, while
//! `appearance.dialog_windows` is on (`dialog_windows.rs`, kind `Lister`). As Total Commander's
//! Lister the window is modeless and not owned by the main window: the panels work beside it and
//! it can lie behind the main window. It takes the keys while it has the keyboard; Escape, F10
//! and its X close that Lister only. Every Lister opens where the last one was
//! (`window.dialog.lister`). Quick View stays in its panel.

use crate::{
    app::{App, Effect, Message, dialog_windows::DialogWindow},
    i18n::t,
};

impl App {
    /// Whether the top Lister lies over the main window and has its keys: one is open and the
    /// Listers have no windows of their own.
    pub(in crate::app) fn lister_over_main(&self) -> bool {
        self.viewers
            .open
            .last()
            .is_some_and(|lister| !self.windowed(DialogWindow::Lister(lister.id)))
    }

    /// Whether a Lister has the keys - over the main window, or in its own window while that has
    /// the keyboard. Escape and F10 close it then (`escape.rs`, `quit.rs`).
    pub(in crate::app) fn lister_has_keys(&self) -> bool {
        self.viewers
            .open
            .iter()
            .any(|lister| self.has_keys(DialogWindow::Lister(lister.id)))
    }

    /// The windows the open Listers want, the oldest first.
    pub(in crate::app) fn lister_windows(&self) -> impl Iterator<Item = DialogWindow> + '_ {
        self.viewers
            .open
            .iter()
            .map(|lister| DialogWindow::Lister(lister.id))
    }

    /// The input of Lister `id`'s window while it has the keyboard: its keys; a click is its own.
    pub(in crate::app) fn lister_input(&mut self, id: u64, message: &Message) -> Vec<Effect> {
        match message {
            Message::Key(chord) => self.lister_key(id, chord, false),
            Message::KeyCaptured(chord) => self.lister_key(id, chord, true),
            _ => Vec::new(),
        }
    }

    /// The title of Lister `id`'s window: as Total Commander's, "Lister" and the file.
    pub(in crate::app) fn lister_title(&self, id: u64) -> String {
        self.viewers
            .open
            .iter()
            .find(|lister| lister.id == id)
            .map_or_else(String::new, |lister| {
                t!("viewer.title", path = lister.path.path.display())
            })
    }
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;
