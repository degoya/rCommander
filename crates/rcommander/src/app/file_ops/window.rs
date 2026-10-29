//! The file operations as their windows of their own draw them (RC-080-23, stage 2): the card's
//! content without its head - a dialog with its prompt first, a job's question, the progress
//! window -, the short title for the title bar, and the field the dialog's window gives the
//! keyboard. Which window is open when is `dialog_windows/file_ops.rs`.

use std::time::Instant;

use iced::{
    Element, Task,
    widget::{Id, column, operation},
};
use rc_engine::{FileDialog, Question};

use super::{FIELD_ID, question, view};
use crate::{
    app::{App, Message, dialog_windows::DialogWindow},
    i18n::t,
    theme::Tokens,
};

/// The window of the dialog `kind` opens: copy and move, a new folder or text file, delete;
/// none for Shift+F6, which renames in the list.
pub(in crate::app) fn dialog_window(kind: FileDialog) -> Option<DialogWindow> {
    match kind {
        FileDialog::Copy | FileDialog::CopyHere | FileDialog::Move => Some(DialogWindow::Transfer),
        FileDialog::Mkdir | FileDialog::NewText => Some(DialogWindow::NewItem),
        FileDialog::Delete | FileDialog::DeletePermanent => Some(DialogWindow::Delete),
        FileDialog::Rename => None,
    }
}

/// Gives the field `id` the keys, with its first `select` characters selected (all of them
/// when `None`), so typing replaces them.
pub(super) fn focus_task(id: &'static str, select: Option<usize>) -> Task<Message> {
    let id = Id::new(id);
    let select = match select {
        Some(end) => operation::select_range(id.clone(), 0, end),
        None => operation::select_all(id.clone()),
    };
    Task::batch([operation::focus(id), select])
}

impl App {
    /// What the window of `kind` shows; `None` once its dialog, question or job is over.
    pub(in crate::app) fn file_ops_body(
        &self,
        kind: DialogWindow,
        tokens: &'static Tokens,
    ) -> Option<Element<'_, Message>> {
        let ops = &self.file_ops;
        match kind {
            DialogWindow::Progress => {
                let job = ops.jobs.foreground()?;
                let now = Instant::now();
                let figures = (ops.jobs.totals(job.id()), ops.jobs.timing(job.id(), now));
                Some(view::progress_card(job, figures, tokens).1)
            }
            kind if kind.asks() => {
                let job = ops.jobs.asking()?;
                (self.question_window() == Some(kind)).then(|| question::card(job, tokens).1)
            }
            kind => {
                let dialog = ops.dialog.as_ref()?;
                if dialog_window(dialog.kind) != Some(kind) {
                    return None;
                }
                // The card's title is the field's prompt; the title bar has the dialog's name.
                let (prompt, body) = view::dialog_card(dialog, tokens);
                Some(column![prompt, body].spacing(10).into())
            }
        }
    }

    /// The title bar of the window of `kind`: the dialog's name, the question's, what the job
    /// does.
    pub(in crate::app) fn file_ops_title(&self, kind: DialogWindow) -> &'static str {
        let ops = &self.file_ops;
        match kind {
            DialogWindow::Progress => ops
                .jobs
                .foreground()
                .map_or("", |job| view::kind_title(&job.snapshot.kind)),
            DialogWindow::Conflict | DialogWindow::JobError => {
                match ops
                    .jobs
                    .asking()
                    .and_then(|job| job.snapshot.question.as_ref())
                {
                    Some(Question::Conflict { resumable, .. }) if *resumable => {
                        t!("ops.conflict.title_part")
                    }
                    Some(Question::Conflict { .. }) => t!("ops.conflict.title"),
                    Some(Question::Error(_)) => t!("ops.error.title"),
                    Some(Question::Password { .. }) | None => "",
                }
            }
            _ => match ops.dialog.as_ref().map(|dialog| dialog.kind) {
                Some(FileDialog::Copy | FileDialog::CopyHere) => t!("ops.window.copy"),
                Some(FileDialog::Move) => t!("ops.window.move"),
                Some(FileDialog::Mkdir) => t!("ops.window.mkdir"),
                Some(FileDialog::NewText) => t!("ops.window.new_text"),
                Some(FileDialog::Delete) => t!("ops.window.delete"),
                Some(FileDialog::DeletePermanent) => t!("ops.window.delete_permanent"),
                Some(FileDialog::Rename) | None => "",
            },
        }
    }

    /// The field the dialog's window of `kind` gives the keyboard when it shows, with the part
    /// of it selected the card selects (Shift+F5: the name without its extension); nothing for
    /// a question, the progress window and a delete.
    pub(in crate::app) fn file_ops_focus(&self, kind: DialogWindow) -> Task<Message> {
        match &self.file_ops.dialog {
            Some(dialog) if dialog.has_field() && dialog_window(dialog.kind) == Some(kind) => {
                focus_task(FIELD_ID, dialog.selection())
            }
            _ => Task::none(),
        }
    }
}
