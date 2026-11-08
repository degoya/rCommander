//! The name lists of the multi-rename dialog and its next step (RC-100-96), Total Commander's F10
//! menu of the tool: "Load names from file", names from the clipboard, "Edit names" in the
//! editor, "Load rename results for next step". In rCommander F10 closes every dialog
//! (RC-080-17), so the commands sit on the "More..." row (Alt+M, Shift+F10) and F5 is the next
//! step.
//!
//! A name list stands in for the masks - they are locked until it is cleared - and the engine
//! checks every name as any new name, an entry without one being a conflict. Edit names writes
//! the new names into a file in the system's temporary folder and starts `files.editor` on it;
//! the editor is not waited for (Notepad on Windows 11 returns at once): the person saves and
//! takes the names with "Take names", and the file is removed. The next step asks the engine for
//! the entries' names after the run that went through and puts the rule back to `<Default>`;
//! every step is a run of its own in the undo log, undone one after the other.
//!
//! The clipboard `[X]` reads is read here as well: again whenever a rule field changes (Total
//! Commander keeps it until the mask changes), before the preview.

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::NaiveDateTime;
use iced::{
    Alignment, Element,
    widget::{column, row},
};

use super::{
    Answer, Call, Field, MrMsg, MultiRenameDialog, PresetChoice, Status,
    controls::{indented, small_button, wrap},
    failure,
};
use crate::{
    app::{
        App, Effect, Message,
        browse::{BrowseMsg, PathField},
    },
    i18n::{error_text, t},
    platform::file_dialog,
    theme::{fit::line, layout},
};

/// How many runs back the dialog keeps the names for its undo, as many as the engine undoes.
const MAX_UNDO_NAMES: usize = 32;

/// The name lists' messages.
#[derive(Debug, Clone)]
pub enum NamesMsg {
    /// "More..." (Alt+M, Shift+F10): the row of the list commands shown or hidden.
    Toggle,
    /// A names file chosen: its path.
    File(String),
    /// Names from the clipboard: it is read.
    FromClipboard,
    /// The clipboard's text, for `[X]` or as names.
    Clipboard(ClipboardUse, Option<String>),
    /// Edit names: the new names into a file for the editor.
    Edit,
    /// "Take names": the edited file read back.
    Take,
    /// The edited file dropped.
    Cancel,
    /// The name list given up: the masks again.
    Clear,
    /// F5: the next step after a run that went through.
    NextStep,
}

/// What the clipboard's text was read for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardUse {
    /// `[X]` in a mask; the preview after it with the typing's pause or without.
    Mask { debounce: bool },
    /// A name list, one name per line.
    Names,
}

impl MrMsg {
    /// Whether the message changes the rule: the moment `[T3]` reads.
    pub(super) fn changes_options(&self) -> bool {
        match self {
            Self::Text(field, _) => !matches!(field, Field::PresetName | Field::LogPath),
            Self::Flag(..) | Self::Case(_) | Self::Insert(_) | Self::Preset(_) => true,
            _ => false,
        }
    }
}

/// The local time now, as the date placeholders read times.
pub(super) fn now() -> NaiveDateTime {
    chrono::Local::now().naive_local()
}

impl MultiRenameDialog {
    /// The new name of every row, the old one where a row has none: what Edit names writes.
    fn new_names(&self) -> Vec<String> {
        (0..self.list.len())
            .filter_map(|index| self.list.row(index))
            .map(|(old, item)| item.map_or(old, |item| item.new.as_str()).to_owned())
            .collect()
    }

    /// F5 is offered: the last run went through and nothing runs now.
    pub fn can_next_step(&self) -> bool {
        !self.busy() && self.next_names.is_some()
    }

    /// The names before a run that went through: what Undo brings back and F5 moves on from.
    pub(super) fn ran(&mut self, before: Vec<String>) {
        if self.undo_names.len() == MAX_UNDO_NAMES {
            self.undo_names.remove(0);
        }
        self.next_names = Some(before.clone());
        self.undo_names.push(before);
    }
}

impl App {
    pub(super) fn mr_names(&mut self, message: NamesMsg) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        let idle = !dialog.busy() && dialog.edited.is_none();
        match message {
            NamesMsg::Toggle => dialog.more = !dialog.more,
            NamesMsg::File(path) if idle => {
                let path = PathBuf::from(path);
                return vec![Effect::MultiRename(Call::ReadNames {
                    path,
                    remove: false,
                })];
            }
            NamesMsg::FromClipboard if idle => return vec![read_clipboard(ClipboardUse::Names)],
            NamesMsg::Clipboard(ClipboardUse::Mask { debounce }, text) => {
                dialog.clipboard = text;
                return self.mr_refresh(debounce);
            }
            NamesMsg::Clipboard(ClipboardUse::Names, text) => {
                match rc_engine::names_from_text(text.as_deref().unwrap_or_default()) {
                    Ok(names) if !names.is_empty() => return self.mr_set_names(names),
                    Ok(_) => {
                        dialog.status =
                            Status::Error(t!("multi_rename.clipboard_no_names").to_owned());
                    }
                    Err(error) => dialog.status = Status::Error(error_text(error.code())),
                }
            }
            NamesMsg::Edit if idle => {
                let file = format!("rcommander-names-{}-{:x}.txt", std::process::id(), stamp());
                return vec![Effect::MultiRename(Call::EditNames {
                    path: std::env::temp_dir().join(file),
                    names: dialog.new_names(),
                })];
            }
            NamesMsg::Take => {
                if let Some(path) = dialog.edited.take() {
                    return vec![Effect::MultiRename(Call::ReadNames { path, remove: true })];
                }
            }
            NamesMsg::Cancel => return self.mr_discard_edit(),
            NamesMsg::Clear => {
                dialog.form.names = None;
                dialog.changed_at = now();
                return self.mr_mask_changed(false);
            }
            NamesMsg::NextStep => {
                if let Some(names) = dialog.next_names.clone().filter(|_| dialog.can_next_step()) {
                    return vec![Effect::MultiRename(Call::NextStep(names))];
                }
            }
            // Refused while a run is asked for or the editor's file waits.
            NamesMsg::File(_) | NamesMsg::FromClipboard | NamesMsg::Edit => {}
        }
        Vec::new()
    }

    /// A name list's answer, the editor's or the next step's.
    pub(super) fn mr_names_answer(&mut self, answer: Answer) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        match answer {
            Answer::Names(Ok(names)) if names.is_empty() => {
                dialog.status = Status::Error(t!("multi_rename.file_no_names").to_owned());
            }
            Answer::Names(Ok(names)) => return self.mr_set_names(names),
            Answer::Names(Err(error)) | Answer::Editing(Err(error)) => {
                dialog.status = Status::Error(t!("multi_rename.names_failed", error = error));
            }
            Answer::Editing(Ok(path)) => {
                dialog.edited = Some(path);
                dialog.status = Status::Info(t!("multi_rename.editing").to_owned());
            }
            Answer::NextStep(Ok(names)) => {
                dialog.next_names = None;
                dialog.list.set_names(names);
                dialog.load(PresetChoice::Default);
                dialog.changed_at = now();
                dialog.status = Status::Info(t!("multi_rename.next_step_ready").to_owned());
                return self.mr_mask_changed(false);
            }
            Answer::NextStep(Err(code)) => dialog.status = failure(code),
            _ => {}
        }
        Vec::new()
    }

    /// `names` stand in for the masks from now on.
    fn mr_set_names(&mut self, names: Vec<String>) -> Vec<Effect> {
        let Some(dialog) = self.multi_rename.as_mut() else {
            return Vec::new();
        };
        dialog.form.names = Some(names);
        dialog.changed_at = now();
        dialog.status = Status::None;
        self.mr_refresh(false)
    }

    /// A rule field changed: the clipboard read again when a mask reads it, then the preview.
    pub(super) fn mr_mask_changed(&mut self, debounce: bool) -> Vec<Effect> {
        let reads = self
            .multi_rename
            .as_ref()
            .is_some_and(|dialog| dialog.form.reads_clipboard());
        if reads {
            return vec![read_clipboard(ClipboardUse::Mask { debounce })];
        }
        self.mr_refresh(debounce)
    }

    /// The editor's file of names not taken, removed: Cancel, or the dialog closed.
    pub(super) fn mr_discard_edit(&mut self) -> Vec<Effect> {
        let edited = self
            .multi_rename
            .as_mut()
            .and_then(|dialog| dialog.edited.take());
        edited
            .map(|path| Effect::MultiRename(Call::Discard(path)))
            .into_iter()
            .collect()
    }
}

fn read_clipboard(usage: ClipboardUse) -> Effect {
    let read = iced::clipboard::read()
        .map(move |text| wrap(MrMsg::Names(NamesMsg::Clipboard(usage, text))));
    Effect::Task(read)
}

/// Makes the editor's file name unique within the process.
fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos())
}

/// "More..." and, when open, the list commands; under it the name list in use and the editor's
/// file waiting to be taken.
pub(super) fn more_row(dialog: &MultiRenameDialog) -> Element<'_, Message> {
    let names = |message| Some(MrMsg::Names(message));
    let mut commands = row![small_button(
        t!("multi_rename.more"),
        names(NamesMsg::Toggle)
    )]
    .spacing(6)
    .align_y(Alignment::Center);
    if dialog.more {
        let idle = !dialog.busy() && dialog.edited.is_none();
        if file_dialog::AVAILABLE {
            let open = Message::Browse(BrowseMsg::Open(PathField::RenameNames, String::new()));
            commands = commands.push(layout::secondary_button(
                t!("multi_rename.names_from_file"),
                idle.then_some(open),
            ));
        }
        commands = commands
            .push(small_button(
                t!("multi_rename.names_from_clipboard"),
                names(NamesMsg::FromClipboard).filter(|_| idle),
            ))
            .push(small_button(
                t!("multi_rename.edit_names"),
                names(NamesMsg::Edit).filter(|_| idle),
            ))
            .push(small_button(
                t!("multi_rename.next_step"),
                names(NamesMsg::NextStep).filter(|_| dialog.can_next_step()),
            ));
    }
    let mut state = row![].spacing(6).align_y(Alignment::Center);
    if let Some(list) = &dialog.form.names {
        state = state
            .push(line(t!("multi_rename.names_in_use", count = list.len())))
            .push(small_button(
                t!("multi_rename.names_clear"),
                names(NamesMsg::Clear),
            ));
    }
    if dialog.edited.is_some() {
        state = state
            .push(line(t!("multi_rename.edited_names")))
            .push(small_button(
                t!("multi_rename.take_names"),
                names(NamesMsg::Take),
            ))
            .push(small_button(
                t!("multi_rename.cancel_edit"),
                names(NamesMsg::Cancel),
            ));
    }
    indented(column![commands, state].spacing(6))
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
