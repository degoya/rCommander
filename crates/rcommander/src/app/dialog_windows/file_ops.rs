//! The file operations in windows of their own (RC-080-23, stage 2; owner 2026-10-19): the
//! dialogs of F5, F6 and Shift+F5 (`Transfer`), F7 and Shift+F4 (`NewItem`), F8 and Shift+F8
//! (`Delete`), a job's conflict and error questions (`Conflict`, `JobError`) and the progress
//! window (`Progress`), each modal to the main window as in Total Commander. What one card over
//! the main window showed at a time - a question first, else the dialog, else the progress - is
//! now a window each: a job's question on top of the dialog or the progress window beside it,
//! and of another modal dialog's window too - a background job's question does not wait for it.
//! What each window draws is `file_ops/window.rs`.
//!
//! The keys: a file operation's window has them before the other dialogs while no other modal
//! dialog's window has the keyboard ([`App::file_ops_first`]); of its windows the question has
//! them but while the dialog's or the progress window has the keyboard
//! ([`App::job_question_has_keys`]). So Escape, F10 and the title bar's X do what Escape does in
//! that window: in the question's they cancel the question, in the dialog's they close the
//! dialog, in the progress window they send the job to the background.

use rc_engine::Question;

use super::{DialogWindow, own::is_input};
use crate::app::{App, Message, file_ops::dialog_window};

impl App {
    /// The file operations' windows the state shows while `appearance.dialog_windows` is on, the
    /// question first: a job's conflict or error, the open dialog, the progress window of the job
    /// in the foreground - an archive's password is a small question's window over it
    /// (`questions.rs`).
    pub(super) fn file_ops_windows(&self) -> Vec<DialogWindow> {
        if !self.dialog_windows.own {
            return Vec::new();
        }
        let ops = &self.file_ops;
        let mut open: Vec<DialogWindow> = self.question_window().into_iter().collect();
        open.extend(
            ops.dialog
                .as_ref()
                .and_then(|dialog| dialog_window(dialog.kind)),
        );
        if ops.jobs.foreground().is_some() {
            open.push(DialogWindow::Progress);
        }
        open
    }

    /// The window of the question a job asks, while the file operations have windows of their
    /// own; an archive's password is a small question's (`questions.rs`).
    pub(in crate::app) fn question_window(&self) -> Option<DialogWindow> {
        if !self.dialog_windows.own {
            return None;
        }
        match self.file_ops.jobs.asking()?.snapshot.question.as_ref()? {
            Question::Conflict { .. } => Some(DialogWindow::Conflict),
            Question::Error(_) => Some(DialogWindow::JobError),
            Question::Password { .. } => None,
        }
    }

    /// Whether `message` goes to the file operations before every other dialog: input, while one
    /// of their windows is open and no other modal dialog's window has the keyboard - there a
    /// background job's question waits until it is clicked.
    pub(in crate::app) fn file_ops_first(&self, message: &Message) -> bool {
        is_input(message)
            && !self.file_ops_windows().is_empty()
            && !self
                .dialog_windows
                .focused_kind()
                .is_some_and(|kind| kind.modal() && !kind.file_ops())
    }

    /// Whether a job's question has the keys: as a card always, in its window but while another
    /// modal dialog's window has the keyboard - the dialog's, the progress window, another
    /// dialog's.
    pub(in crate::app) fn job_question_has_keys(&self) -> bool {
        self.question_window().is_none()
            || !self
                .dialog_windows
                .focused_kind()
                .is_some_and(|kind| kind.modal() && !kind.asks())
    }
}

#[cfg(test)]
#[path = "file_ops_tests.rs"]
mod tests;
