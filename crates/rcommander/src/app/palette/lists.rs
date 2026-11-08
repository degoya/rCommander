//! The menu entries of RC-100-92 that need the person first: Mark > "Save selection to file...",
//! "Load selection from file..." and "Load selection from clipboard", Commands > "Load list
//! file...". The system's file dialog asks for the file (its own save dialog asks before it
//! replaces one), the clipboard is read by the window - the engine never reads it -, and the
//! command runs for the active panel.

use std::path::PathBuf;

use rc_engine::Command;

use super::PaletteMsg;
use crate::app::{
    App, Effect, Message,
    browse::{BrowseMsg, PathField},
};

/// A menu entry that asks before its command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListDialog {
    SelectionToFile,
    SelectionFromFile,
    SelectionFromClipboard,
    LoadList,
}

/// What the dialogs answer.
#[derive(Debug, Clone)]
pub enum ListsMsg {
    /// A file was chosen for `dialog`.
    Picked(ListDialog, PathBuf),
    /// The clipboard's text, if it held one.
    Clipboard(Option<String>),
}

/// The message a chosen `path` sends for `dialog`.
pub fn picked(dialog: ListDialog, path: String) -> Message {
    Message::Palette(PaletteMsg::Lists(ListsMsg::Picked(
        dialog,
        PathBuf::from(path),
    )))
}

/// The command `dialog` runs with the file `path`, for the active panel.
fn command(dialog: ListDialog, path: PathBuf) -> Option<Command> {
    let path = path.into_os_string().into_string().ok()?;
    match dialog {
        // The system's save dialog asked before it chose a file that is there.
        ListDialog::SelectionToFile => Some(Command::SelectionToFile {
            panel: None,
            path,
            overwrite: true,
        }),
        ListDialog::SelectionFromFile => Some(Command::SelectionFromFile { panel: None, path }),
        ListDialog::LoadList => Some(Command::LoadList { panel: None, path }),
        ListDialog::SelectionFromClipboard => None,
    }
}

impl App {
    /// A menu entry of [`ListDialog`]: the file dialog, or the clipboard read.
    pub(in crate::app) fn open_list_dialog(&mut self, dialog: ListDialog) -> Vec<Effect> {
        let field = match dialog {
            ListDialog::SelectionToFile => PathField::SelectionSave,
            ListDialog::SelectionFromFile => PathField::SelectionLoad,
            ListDialog::LoadList => PathField::ListFile,
            ListDialog::SelectionFromClipboard => {
                let read = iced::clipboard::read()
                    .map(|text| Message::Palette(PaletteMsg::Lists(ListsMsg::Clipboard(text))));
                return vec![Effect::Task(read)];
            }
        };
        self.apply(Message::Browse(BrowseMsg::Open(field, String::new())))
    }

    pub(in crate::app) fn on_lists(&mut self, message: ListsMsg) -> Vec<Effect> {
        let command = match message {
            ListsMsg::Picked(dialog, path) => command(dialog, path),
            ListsMsg::Clipboard(text) => {
                text.map(|text| Command::SelectionFromText { panel: None, text })
            }
        };
        command
            .map(|command| Effect::Dispatch(self.active, vec![command]))
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
#[path = "lists_tests.rs"]
mod tests;
