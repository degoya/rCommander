//! The server files edited through watched copies, the window's side (RC-110-01): the question
//! after a save (Upload / Not now, with the box "Upload changes without asking"), the questions
//! about a changed or gone server file and a file written in place, the offer of a change kept
//! at the last quit (Upload / Discard / Later), the list "Server files being edited" (the status
//! bar's "N server files open", the palette's `remote_edit.window`) with Upload now and Done,
//! and Done's question when the copy holds a change. The edits are the engine's; the window
//! reads them again on each [`EngineEvent::RemoteEdit`].
//!
//! It hangs off the quit (`quit.rs`): quitting names these copies, and `app.rs` has no room for
//! a dialog of its own (500 lines). Each question is a small question of the window, one at a
//! time, the oldest edit's first; Escape answers as the safe button does - Not now, Cancel,
//! Later, Keep.

mod view;

use rc_engine::{
    Chord, Command, EngineEvent, RemoteEditId, RemoteEditInfo, RemoteEditQuestion, UploadChoice,
};

pub use view::status;

use super::QuitMsg;
use crate::{
    app::{
        App, Effect, Message,
        escape::{enter, escape},
        field_walk,
    },
    i18n::t,
};

/// The remote edits as the window shows them.
#[derive(Debug, Default)]
pub struct RemoteEditUi {
    /// The edits as the engine listed them last.
    pub edits: Vec<RemoteEditInfo>,
    /// The box "Upload changes without asking" of the question after a save.
    pub always: bool,
    /// The list "Server files being edited" while it is open: its cursor's row.
    pub list: Option<usize>,
    /// Done on a copy that holds a change: asked first.
    pub discard: Option<RemoteEditId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteEditMsg {
    /// A button that uploads: Upload, Overwrite, Upload as copy, Overwrite in place, Upload now.
    Upload(RemoteEditId, UploadChoice),
    /// Not now, Cancel, Later.
    Skip(RemoteEditId),
    /// Discard of a kept change.
    Close(RemoteEditId),
    /// The box "Upload changes without asking".
    Always(bool),
    /// The status bar's "N server files open".
    OpenList,
    /// A row of the list clicked, or the arrows.
    Select(usize),
    /// Done on the list's row.
    Done(RemoteEditId),
    /// Done's question: Discard (`true`) or Keep.
    Discard(bool),
    /// Close of the list.
    CloseList,
}

/// How many copies the quit's question names; the rest are counted (as Total Commander 11).
const NAMED: usize = 3;

/// `message` as the window's message.
pub fn wrap(message: RemoteEditMsg) -> Message {
    Message::Quit(QuitMsg::RemoteEdit(message))
}

/// The copies `edits` as the quit's question names them: the first three with their server,
/// then how many more.
pub fn named(edits: &[&RemoteEditInfo]) -> String {
    let mut names: Vec<String> = edits
        .iter()
        .take(NAMED)
        .map(|edit| {
            t!(
                "quit.remote_edit",
                name = edit.name.as_str(),
                server = edit.server.as_str()
            )
        })
        .collect();
    if edits.len() > NAMED {
        names.push(t!("quit.remote_edits_more", count = edits.len() - NAMED));
    }
    names.join(", ")
}

impl RemoteEditUi {
    /// The edit whose question is asked now: the oldest that waits for one.
    pub fn asking(&self) -> Option<&RemoteEditInfo> {
        self.edits.iter().find(|edit| edit.question.is_some())
    }

    /// Whether a question about an edit is open: one the engine asks, or Done's.
    pub fn asks(&self) -> bool {
        self.discard.is_some() || self.asking().is_some()
    }

    /// The edit on the list's cursor.
    pub fn cursor(&self) -> Option<&RemoteEditInfo> {
        self.list.and_then(|row| self.edits.get(row))
    }

    /// The question of `id` is answered: it closes at once, so a second click sends nothing.
    fn answered(&mut self, id: RemoteEditId) -> Option<RemoteEditQuestion> {
        self.edits
            .iter_mut()
            .find(|edit| edit.id == id)
            .and_then(|edit| edit.question.take())
    }
}

impl App {
    /// The remote edits' share of every message: the engine's events, and while a question or
    /// the list is open every key and click. `None` for a message it leaves to the rest.
    pub(in crate::app) fn remote_edit_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        match message {
            Message::Engine(EngineEvent::RemoteEdit { .. }) => {
                self.read_remote_edits();
                Some(Vec::new())
            }
            Message::Engine(EngineEvent::RemoteEditWindow) => {
                self.read_remote_edits();
                self.quit.remote.list = Some(0);
                Some(Vec::new())
            }
            _ if !self.quit.remote.asks() && self.quit.remote.list.is_none() => None,
            // Tab and Shift+Tab walk the buttons, the box and the list (RC-080-16).
            Message::Key(chord) if field_walk::tab(chord).is_some() => field_walk::walk_key(chord),
            Message::Key(chord) if *chord == escape() => Some(self.remote_edit_escape()),
            // Enter on no focused control takes the safe default; a focused one took it.
            Message::Key(chord) if *chord == enter() => Some(self.remote_edit_enter()),
            Message::Key(chord) if !self.quit.remote.asks() => Some(self.remote_edit_step(chord)),
            Message::Key(_) | Message::KeyCaptured(_) | Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// The edits read again from the engine; the list's cursor stays on the list.
    pub(in crate::app) fn read_remote_edits(&mut self) {
        let ui = &mut self.quit.remote;
        ui.edits = self.engine.remote_edits();
        if let Some(row) = ui.list.as_mut() {
            *row = (*row).min(ui.edits.len().saturating_sub(1));
        }
        if ui
            .discard
            .is_some_and(|id| !ui.edits.iter().any(|edit| edit.id == id))
        {
            ui.discard = None;
        }
    }

    /// A button, the box or a row.
    pub(in crate::app) fn on_remote_edit(&mut self, message: RemoteEditMsg) -> Vec<Effect> {
        if message == RemoteEditMsg::OpenList {
            self.read_remote_edits();
            self.quit.remote.list = Some(0);
            return Vec::new();
        }
        let ui = &mut self.quit.remote;
        let command = match message {
            RemoteEditMsg::Upload(id, choice) => {
                let asked = ui.answered(id);
                // The box counts with Upload on the question after a save.
                let always = std::mem::take(&mut ui.always)
                    && asked == Some(RemoteEditQuestion::Upload)
                    && choice == UploadChoice::Upload;
                Command::UploadRemoteEdit { id, choice, always }
            }
            RemoteEditMsg::Skip(id) => {
                ui.always = false;
                ui.answered(id);
                Command::SkipRemoteEdit { id }
            }
            RemoteEditMsg::Close(id) => {
                ui.answered(id);
                Command::CloseRemoteEdit { id }
            }
            RemoteEditMsg::Done(id) => {
                let unsaved = ui
                    .edits
                    .iter()
                    .any(|edit| edit.id == id && edit.state.unsaved());
                if unsaved {
                    ui.discard = Some(id);
                    return Vec::new();
                }
                Command::CloseRemoteEdit { id }
            }
            RemoteEditMsg::Discard(go) => match ui.discard.take() {
                Some(id) if go => Command::CloseRemoteEdit { id },
                _ => return Vec::new(),
            },
            RemoteEditMsg::Always(on) => {
                ui.always = on;
                return Vec::new();
            }
            RemoteEditMsg::Select(row) => {
                if row < ui.edits.len() {
                    ui.list = Some(row);
                }
                return Vec::new();
            }
            RemoteEditMsg::CloseList => {
                ui.list = None;
                return Vec::new();
            }
            RemoteEditMsg::OpenList => return Vec::new(),
        };
        vec![Effect::Dispatch(self.active, vec![command])]
    }

    /// Escape: the safe answer of the open question, else the list closes.
    fn remote_edit_escape(&mut self) -> Vec<Effect> {
        let ui = &self.quit.remote;
        let message = if ui.discard.is_some() {
            RemoteEditMsg::Discard(false)
        } else if let Some(edit) = ui.asking() {
            RemoteEditMsg::Skip(edit.id)
        } else {
            RemoteEditMsg::CloseList
        };
        self.on_remote_edit(message)
    }

    /// Enter on no focused control: the question's default - Upload after a save and for a
    /// kept change, the safe answer otherwise; nothing in the list.
    fn remote_edit_enter(&mut self) -> Vec<Effect> {
        let ui = &self.quit.remote;
        let message = match (ui.discard, ui.asking()) {
            (Some(_), _) => RemoteEditMsg::Discard(false),
            (None, Some(edit)) => match edit.question {
                Some(RemoteEditQuestion::Upload | RemoteEditQuestion::Kept) => {
                    RemoteEditMsg::Upload(edit.id, UploadChoice::Upload)
                }
                _ => RemoteEditMsg::Skip(edit.id),
            },
            (None, None) => return Vec::new(),
        };
        self.on_remote_edit(message)
    }

    /// Up and Down in the list move its cursor; other keys go nowhere.
    fn remote_edit_step(&mut self, chord: &Chord) -> Vec<Effect> {
        let ui = &mut self.quit.remote;
        let (Some(row), len) = (ui.list, ui.edits.len()) else {
            return Vec::new();
        };
        if Chord::plain("ArrowUp").as_ref() == Some(chord) {
            ui.list = Some(row.saturating_sub(1));
        } else if Chord::plain("ArrowDown").as_ref() == Some(chord) {
            ui.list = Some((row + 1).min(len.saturating_sub(1)));
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "remote_edit_tests.rs"]
mod tests;
