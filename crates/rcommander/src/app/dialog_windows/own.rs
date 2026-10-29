//! Which window the keys belong to while large dialogs have windows of their own (RC-080-15).
//!
//! A modeless dialog's window - the connection manager, find files, compare, the checksum
//! results, the login-script editor, each Lister - takes the keys and clicks only while it has
//! the keyboard, which the system's focus events tell
//! (`QueueMsg::Focused`, kept in the books); then the keys go to that dialog alone, never to the
//! panels ([`App::own_window_intercept`]). While the main window has the keyboard, those dialogs
//! take none of its keys ([`App::reaches`]), so the panels work beside them, as beside the
//! queue's window (`queue_window/own.rs`). A modal one - the settings, multi-rename, the
//! standalone dialogs (RC-080-23) - has the keys whenever it is open: the main window takes none
//! then, nor do its own layers - a file operation's cards, the drag question, the queue's card
//! ([`App::main_reaches`]); the file operations' own windows take theirs first
//! (`dialog_windows/file_ops.rs`). The small questions come before all of this and draw in the
//! window that has the keyboard (`dialog_windows.rs`).

use iced::{Task, window};
use rc_engine::{
    EngineEvent,
    settings::{DIALOG_WINDOWS_KEY, stored_bool},
};

use super::DialogWindow;
use crate::app::{App, Effect, Message};

/// Whether `message` is input - a key or a click - which goes to the window that has the
/// keyboard; every other message is the app's whichever window shows what.
pub fn is_input(message: &Message) -> bool {
    matches!(
        message,
        Message::Key(_) | Message::KeyCaptured(_) | Message::Press
    )
}

impl App {
    /// A changed `appearance.dialog_windows`, and the input of a modeless dialog's window while
    /// it has the keyboard: that dialog's, the layers over it first - the "Run command" dialog
    /// and the login-script editor over the connection manager. Its window's input is never the
    /// panels', so a key it does not use goes nowhere.
    pub(in crate::app) fn own_window_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        if let Message::Engine(EngineEvent::SettingChanged { key }) = message
            && key == DIALOG_WINDOWS_KEY
        {
            return Some(vec![Effect::Task(self.dialog_windows_task())]);
        }
        if !is_input(message) {
            return None;
        }
        let kind = self
            .dialog_windows
            .focused_kind()
            .filter(|kind| !kind.modal() && kind.takes_questions())
            .filter(|kind| self.windowed(*kind))?;
        // The login-script editor and "Run command" in windows of their own take their input
        // there.
        let editor_apart = self.windowed(DialogWindow::LoginScripts);
        let exec_apart = self.windowed(DialogWindow::SshExec);
        let taken = match kind {
            DialogWindow::Connections => {
                let exec = if exec_apart {
                    None
                } else {
                    self.ssh_exec_intercept(message)
                };
                exec.or_else(|| {
                    if editor_apart {
                        None
                    } else {
                        self.login_scripts_intercept(message)
                    }
                })
                .or_else(|| self.connections_intercept(message))
            }
            DialogWindow::Search => self.search_intercept(message),
            DialogWindow::Compare => self.compare_intercept(message),
            DialogWindow::LoginScripts => self.login_scripts_intercept(message),
            DialogWindow::Checksum => match message {
                Message::Key(chord) => Some(self.checksum_key(chord, false)),
                Message::KeyCaptured(chord) => Some(self.checksum_key(chord, true)),
                _ => None,
            },
            DialogWindow::Lister(id) => Some(self.lister_input(id, message)),
            DialogWindow::Settings
            | DialogWindow::Queue
            | DialogWindow::MultiRename
            | DialogWindow::Database
            | DialogWindow::Properties
            | DialogWindow::QuickConnect
            | DialogWindow::Network
            | DialogWindow::Archive
            | DialogWindow::ChecksumForm
            | DialogWindow::Hotlist
            | DialogWindow::SshExec
            | DialogWindow::Transfer
            | DialogWindow::NewItem
            | DialogWindow::Delete
            | DialogWindow::Conflict
            | DialogWindow::JobError
            | DialogWindow::Progress
            // A question's window takes its input before (`questions.rs`).
            | DialogWindow::Question(_) => None,
        };
        Some(taken.unwrap_or_default())
    }

    /// Whether the dialog of `kind` is in a window of its own now.
    pub(in crate::app) fn windowed(&self, kind: DialogWindow) -> bool {
        self.wanted_windows().contains(&kind)
    }

    /// Whether the dialog of `kind` has the keys: lying over the main window, modal, or in its
    /// own window while that has the keyboard. Escape and F10 close it then (`escape.rs`).
    pub(in crate::app) fn has_keys(&self, kind: DialogWindow) -> bool {
        kind.modal() || !self.windowed(kind) || self.dialog_windows.has_focus(kind)
    }

    /// Whether a modal dialog has a window of its own now: the main window is locked.
    pub(in crate::app) fn main_locked(&self) -> bool {
        self.wanted_windows().iter().any(|kind| kind.modal())
    }

    /// Whether `message` may reach the main window's own layers - a file operation's question
    /// or progress, the drag question, the queue's card: anything but input always, input while
    /// no modal dialog's window locks the main window (RC-080-23). There the question waits for
    /// the dialog, and the keys of the dialog's window are the dialog's.
    pub(in crate::app) fn main_reaches(&self, message: &Message) -> bool {
        !is_input(message) || !self.main_locked()
    }

    /// Whether the checksum window has the keys (`has_keys`); the main window's chain sends them
    /// to it then (`App::apply`).
    pub(in crate::app) fn checksum_has_keys(&self) -> bool {
        self.checksum.is_some() && self.has_keys(DialogWindow::Checksum)
    }

    /// Whether "Run command" has the keys: in its window of its own, modal, whenever it is open;
    /// else over the connection manager while the manager has them.
    pub(in crate::app) fn ssh_exec_has_keys(&self) -> bool {
        self.ssh_exec.is_some()
            && (self.windowed(DialogWindow::SshExec) || self.has_keys(DialogWindow::Connections))
    }

    /// Whether the login-script editor has the keys: in its own window while that has the
    /// keyboard, else over the connection manager while the manager has them.
    pub(in crate::app) fn login_scripts_have_keys(&self) -> bool {
        let keys = if self.windowed(DialogWindow::LoginScripts) {
            self.dialog_windows.has_focus(DialogWindow::LoginScripts)
        } else {
            self.has_keys(DialogWindow::Connections)
        };
        self.login_scripts.is_some() && keys
    }

    /// Whether `message` may reach the dialog of `kind` in the main window's chain: anything but
    /// input always, input while the dialog has the keys.
    pub(in crate::app) fn reaches(&self, kind: DialogWindow, message: &Message) -> bool {
        !is_input(message) || self.has_keys(kind)
    }

    /// The window of `kind` to the front, while it is open: its dialog was asked for again.
    pub(in crate::app) fn dialog_window_front(&self, kind: DialogWindow) -> Vec<Effect> {
        self.dialog_windows
            .id_of(kind)
            .map(|id| Effect::Task(window::gain_focus(id)))
            .into_iter()
            .collect()
    }

    /// Reads `appearance.dialog_windows`; at the start and when it changes.
    pub(in crate::app) fn dialog_windows_task(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return Task::none();
        };
        Task::perform(
            async move { stored_bool(&db, DIALOG_WINDOWS_KEY).await },
            Message::DialogWindows,
        )
    }
}

#[cfg(test)]
#[path = "own_tests.rs"]
mod tests;
