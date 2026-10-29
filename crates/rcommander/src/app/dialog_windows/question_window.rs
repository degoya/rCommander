//! The window of a small question (RC-080-23, `questions.rs`): opened owned by the window the
//! question would hide behind, which takes no input while it is open, and closed before that
//! window closes - Windows destroys an owned window with its owner.

use std::time::Instant;

use iced::{Task, window};

use super::{DialogWindow, DialogWindowMsg, questions::Question};
use crate::{
    app::{App, Message, window_state::Geometry},
    platform,
};

impl App {
    /// The window the window of `question` is owned by and locks: the compare window for the
    /// synchronization's confirmation; for every other question the front modal dialog's window
    /// while one is open - the question would lie behind it -, else the main window.
    fn question_owner(&self, question: Question) -> Option<window::Id> {
        let books = &self.dialog_windows;
        if question == Question::SyncConfirm {
            return books.id_of(DialogWindow::Compare);
        }
        books.front_modal().or(books.main())
    }

    /// A modal dialog's window shown after a question's window was (a job's question while the
    /// MCP call is asked): the question's window, once shown, closes now, so in the same round it
    /// opens again over the new window, owned by it - else the dialog would lie in front of the
    /// question that has the keys. Before the windows to open are counted.
    pub(super) fn question_makes_way(&mut self) -> Task<Message> {
        let books = &self.dialog_windows;
        let Some((id, owner)) = books.question_owner else {
            return Task::none();
        };
        let Some(DialogWindow::Question(question)) = books.kind_of(id) else {
            return Task::none();
        };
        let shown = books.visible(DialogWindow::Question(question)).is_some();
        if !shown || self.question_owner(question) == Some(owner) {
            return Task::none();
        }
        self.close_question_window(id)
    }

    /// Shows the window `id` of `question`, made invisible and ready (`App::on_dialog_window`),
    /// owned by and locking [`App::question_owner`]: the main window out of the taskbar first -
    /// an owned window shows only with its owner -, then the question in front with the keyboard
    /// on its first control and its taskbar button flashing until the person looks.
    pub(super) fn show_question_window(
        &mut self,
        id: window::Id,
        question: Question,
        since: Instant,
        reused: bool,
    ) -> Task<Message> {
        let owner = self.question_owner(question);
        self.dialog_windows.question_owner = owner.map(|owner| (id, owner));
        let main = self.dialog_windows.main();
        let restored = main.map_or_else(Task::none, |main| window::minimize(main, false));
        let focus = self.opening_focus(DialogWindow::Question(question));
        let attention = window::request_user_attention(id, Some(window::UserAttention::Critical));
        let shown = DialogWindowMsg::Shown { id, since, reused };
        restored
            .chain(platform::dialog_window::show(id, owner, true))
            .chain(focus)
            .chain(attention)
            .chain(Task::done(Message::DialogWindow(shown)))
    }

    /// Closing window `id`: a question's window closes here; a window that owns the question's -
    /// shown or being shown - closes after it, as Windows would destroy it with its owner; the
    /// question opens again at the next update, owned by the next window (closing windows send
    /// their focus events). `None` for any other window.
    pub(super) fn close_question_first(&mut self, id: window::Id) -> Option<Task<Message>> {
        if let Some(DialogWindow::Question(_)) = self.dialog_windows.kind_of(id) {
            return Some(self.close_question_window(id));
        }
        let (question, owner) = self.dialog_windows.question_owner?;
        if owner != id || question == id {
            return None;
        }
        let first = self.close_question_window(question);
        Some(first.chain(self.close_dialog_window(id)))
    }

    /// Closes the question's window `id` and stores where it was. The window it locked takes
    /// input again first, so Windows gives it the focus: a dialog's window at once, the main
    /// window with the last modal window closing.
    fn close_question_window(&mut self, id: window::Id) -> Task<Message> {
        let owner = self
            .dialog_windows
            .question_owner
            .take_if(|(question, _)| *question == id)
            .map(|(_, owner)| owner);
        let closing = self.dialog_windows.closed(id);
        let main = self.dialog_windows.main();
        let released = match owner {
            Some(owner) if Some(owner) != main => Some(owner),
            _ => main.filter(|_| !self.dialog_windows.modal_open()),
        };
        let stored = self.store_place(closing);
        let closed = platform::dialog_window::close(id, released, released.is_some());
        Task::batch([stored, closed])
    }

    /// Stores where a closing question's window was, for its next opening.
    fn store_place(&self, closing: Option<(DialogWindow, Geometry)>) -> Task<Message> {
        let (Some((kind, geometry)), Some(db)) = (closing, self.db.clone()) else {
            return Task::none();
        };
        Task::future(async move {
            // A place that cannot be stored is only missing at the next opening.
            if let Err(error) = db.set(kind.key(), &geometry).await {
                tracing::warn!(
                    key = kind.key(),
                    %error,
                    "a question window's place was not stored"
                );
            }
        })
        .discard()
    }
}

#[cfg(test)]
#[path = "question_window_tests.rs"]
mod tests;
