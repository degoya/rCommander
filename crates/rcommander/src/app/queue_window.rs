//! The queue window (RC-010-14), after Total Commander's background transfer manager: every job
//! of the operations queue with its kind, source and target, state, progress and lane; the
//! waiting ones moved up, down or to the front, paused or removed; a running one paused or - after
//! a yes - cancelled; everything paused or resumed at once, by one button that changes as the
//! selected job's does; the finished ones cleared.
//!
//! It opens on `queue.window` (Ctrl+Shift+Q, a click on the background bar), which the engine
//! announces ([`EngineEvent::QueueWindow`]); it is one layer under the file operations' dialogs,
//! so a job's conflict question comes up over it and is answered there. What it shows is the
//! queue's own list ([`rc_engine::QueueEntry`]), read again on every job event and on
//! [`EngineEvent::QueueChanged`]; it lists the jobs run at once too, a group of their own, unless
//! `queue.show_at_once` is off, and the background bar previews its order (`shown.rs`,
//! RC-010-25). Every button and key runs a registry command. Drawing is `view.rs`.
//!
//! With `queue.window_own` on (the default, RC-010-14) it is a native window of its own beside the
//! main window, which works on - modeless, unlike the settings' window (`own.rs`,
//! `dialog_windows.rs`); off, it lies over the main window.
//!
//! It shows one of the queue's named lists at a time (RC-010-15, `lists.rs`, drawn by
//! `lists_view.rs`): made, renamed, deleted, started, stopped, saved and loaded there, a job put
//! into another one.
//!
//! Several jobs are selected as in Windows and Total Commander (`selection.rs`, owner 2026-10-18)
//! and moved, put into a list or a new one, or taken off together - after one question when one
//! of them has started -; a list or every list is cleared. The buttons and keys take the
//! selection, a context menu (`menu.rs`, right click, Shift+F10, the context-menu key) offers
//! every action on it (`bulk.rs`).

mod bulk;
mod figures;
mod lists;
mod lists_view;
mod menu;
mod menu_view;
mod own;
mod rows;
mod selection;
mod shown;
mod view;

use std::collections::BTreeSet;

pub use bulk::Confirm;
pub use lists::{ListCall, ListChoice, ListMsg, answered, list_choices};
use lists::{Naming, Note};
pub use menu::{MenuMsg, QueueMenu};
use rc_engine::{
    Chord, Command, DEFAULT_LIST, EngineEvent, JobId, JobState, QueueEntry, QueueList, QueueListId,
    Reorder, SavedList,
};
pub use shown::{Shown, shown};

use super::{App, Effect, Message};

/// The open queue window.
#[derive(Debug, Default)]
pub struct QueueWindow {
    /// The queue as last read, in its order.
    pub entries: Vec<QueueEntry>,
    /// The job of the cursor row; the actions take it while no job is marked.
    pub selected: Option<JobId>,
    /// The marked jobs (`selection.rs`); the actions take them.
    pub marked: BTreeSet<JobId>,
    /// Where Shift+click and Shift+arrows mark from.
    pub anchor: Option<JobId>,
    /// Jobs taken off or a list cleared, waiting for the person's yes.
    pub confirm: Option<Confirm>,
    /// The open context menu.
    pub menu: Option<QueueMenu>,
    /// The list shown; `entries` are its jobs.
    pub list: QueueListId,
    /// The queue's lists, as last read.
    pub lists: Vec<QueueList>,
    /// The saved lists, as last read.
    pub saved: Vec<SavedList>,
    /// The saved list Load and Delete act on.
    pub saved_choice: Option<i64>,
    /// The open name field: a new list, a rename, a save.
    pub naming: Option<Naming>,
    /// A saved list whose deletion waits for the person's yes.
    pub confirm_delete_saved: Option<i64>,
    /// What the last list action did, or why it was refused.
    pub note: Option<Note>,
    /// What the one button for the whole queue does, as last read.
    pub all: AllToggle,
    /// How far the list is scrolled down, as it last reported: the rows around it are built
    /// (`rows.rs`, RC-080-45).
    pub scrolled: f32,
}

/// The one button for the whole queue, like the selected job's own (RC-010-14): "Resume all"
/// once every unfinished job is paused - a job asking a question counts as neither -, else
/// "Pause all", also over an empty queue, where it pauses nothing (it stays a stop of the ring).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AllToggle {
    #[default]
    Pause,
    Resume,
}

/// The button for the whole queue over `entries`, every list's jobs.
pub fn all_toggle(entries: &[QueueEntry]) -> AllToggle {
    let (mut going, mut paused) = (false, false);
    for entry in entries
        .iter()
        .filter(|entry| !entry.job.state.is_finished())
    {
        match entry.job.state {
            JobState::Paused => paused = true,
            JobState::Waiting => {}
            _ => going = true,
        }
    }
    if paused && !going {
        AllToggle::Resume
    } else {
        AllToggle::Pause
    }
}

impl QueueWindow {
    pub fn entry(&self, job: JobId) -> Option<&QueueEntry> {
        self.entries.iter().find(|entry| entry.job.id == job)
    }

    fn selected_entry(&self) -> Option<&QueueEntry> {
        self.entry(self.selected?)
    }

    /// The selection one row up (`-1`) or down (`1`), staying inside the list.
    fn step(&mut self, by: isize) {
        let at = self
            .selected
            .and_then(|job| self.entries.iter().position(|entry| entry.job.id == job));
        let next = match at {
            Some(at) => at
                .saturating_add_signed(by)
                .min(self.entries.len().saturating_sub(1)),
            None => 0,
        };
        self.selected = self.entries.get(next).map(|entry| entry.job.id);
    }
}

#[derive(Debug, Clone)]
pub enum QueueMsg {
    /// A click on a row: its job alone, with Ctrl marked or unmarked, with Shift the range.
    Select(JobId),
    /// A right click on a row: the menu, for the selection - the row's job alone unless marked.
    RowMenu(JobId),
    /// The selected waiting jobs up, down, to the front or to the back of their lanes.
    Move(Reorder),
    Pause(JobId),
    Resume(JobId),
    /// The selected jobs leave the queue; asks first when one of them has started.
    Remove,
    /// Yes to the open question.
    Confirm,
    /// No: everything stays.
    Keep,
    SelectAll,
    /// The context menu.
    Menu(MenuMsg),
    PauseAll,
    ResumeAll,
    ClearFinished,
    /// The job's question comes up first.
    Answer(JobId),
    Close,
    /// The queue's lists (RC-010-15).
    Lists(ListMsg),
    /// A window took the keyboard (`true`) or let go of it (`own.rs`).
    Focused(iced::window::Id, bool),
    /// The setting `queue.window_own`, read.
    Own(bool),
    /// The list scrolled to this offset from its top.
    Scrolled(f32),
}

/// Whether the queue window takes `event`.
pub fn concerns(event: &EngineEvent) -> bool {
    matches!(event, EngineEvent::QueueWindow | EngineEvent::QueueChanged)
}

impl App {
    /// An event of [`concerns`].
    pub(super) fn on_queue_event(&mut self, event: &EngineEvent) -> Vec<Effect> {
        let mut effects = Vec::new();
        if *event == EngineEvent::QueueWindow {
            match self.queue_window {
                None => self.queue_window = Some(QueueWindow::default()),
                // In a window of its own, Ctrl+Shift+Q brings it to the front.
                Some(_) => effects.extend(self.queue_window_front()),
            }
        }
        self.refresh_queue_window();
        effects.extend(self.read_saved_lists());
        effects
    }

    /// Reads the queue again into the background bar and the open window; the selection stays on
    /// its job while it is listed, else goes to the first row.
    pub(super) fn refresh_queue_window(&mut self) {
        let ops = self.engine.ops();
        let entries = ops.entries();
        self.file_ops.bar = shown(&entries, Shown::Bar).cloned().collect();
        let at_once = !self.file_ops.hide_at_once;
        let Some(window) = self.queue_window.as_mut() else {
            return;
        };
        window.lists = ops.lists();
        if !window.lists.iter().any(|list| list.id == window.list) {
            window.list = DEFAULT_LIST;
        }
        let list = window.list;
        window.all = all_toggle(&entries);
        let scope = Shown::List { list, at_once };
        window.entries = shown(&entries, scope).cloned().collect();
        if window.selected_entry().is_none() {
            window.selected = window.entries.first().map(|entry| entry.job.id);
        }
        window.keep_listed();
    }

    /// While the window is open it has the keys, and clicks reach no panel; `None` for a
    /// message it does not take. The file operations' dialogs over it come first.
    pub(super) fn queue_window_intercept(&mut self, message: &Message) -> Option<Vec<Effect>> {
        self.queue_window.as_ref()?;
        // In a window of its own it takes only its own keys (`own.rs`).
        if self.dialog_windows.queue_own {
            return None;
        }
        match message {
            // Tab walks the focus ring; a key a stop of it took is not the window's again.
            Message::Key(chord) if super::field_walk::tab(chord).is_some() => {
                super::field_walk::walk_key(chord)
            }
            Message::KeyCaptured(chord) if super::field_walk::taken(chord) => Some(Vec::new()),
            Message::Key(chord) | Message::KeyCaptured(chord) => Some(self.queue_key(chord)),
            Message::Press => Some(Vec::new()),
            _ => None,
        }
    }

    /// The window's keys: arrows move the cursor, Shift+arrows mark, Space and Insert mark and
    /// move on, Ctrl+A marks all; Ctrl+arrows, Ctrl+Home and Ctrl+End move the selected waiting
    /// jobs, P pauses or resumes, Delete takes the selection off, Enter brings up a question,
    /// Shift+F10 and the context-menu key open the menu, Escape closes.
    fn queue_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let Some(window) = self.queue_window.as_mut() else {
            return Vec::new();
        };
        let plain = chord.is_plain();
        let ctrl = chord.ctrl && !chord.shift && !chord.alt;
        let shift = chord.shift && !chord.ctrl && !chord.alt;
        let key = chord.key.as_str();
        if window.menu.is_some() {
            return self.queue_menu_key(chord);
        }
        if window.naming.is_some() {
            // The name field has the keys; Enter is its own.
            return match (plain, key) {
                (true, "Escape") => self.on_list(ListMsg::NameCancel),
                _ => Vec::new(),
            };
        }
        if window.confirm_delete_saved.is_some() {
            return match (plain, key) {
                (true, "Enter") => self.on_list(ListMsg::ConfirmDeleteSaved),
                (true, "Escape") => self.on_list(ListMsg::KeepSaved),
                _ => Vec::new(),
            };
        }
        if window.confirm.is_some() {
            return match (plain, key) {
                (true, "Enter") => self.on_queue(QueueMsg::Confirm),
                (true, "Escape") => self.on_queue(QueueMsg::Keep),
                _ => Vec::new(),
            };
        }
        let selected = window.selected_entry().map(|entry| entry.job.clone());
        let message = match key {
            "Escape" if plain => QueueMsg::Close,
            "ArrowUp" | "ArrowDown" if plain || shift => {
                let by = if key == "ArrowUp" { -1 } else { 1 };
                if shift {
                    window.extend(by);
                } else {
                    window.step(by);
                    window.anchor = window.selected;
                }
                return Vec::new();
            }
            "ArrowUp" | "ArrowDown" | "Home" | "End" if ctrl => QueueMsg::Move(match key {
                "ArrowUp" => Reorder::Up,
                "ArrowDown" => Reorder::Down,
                "Home" => Reorder::Front,
                _ => Reorder::Back,
            }),
            "a" if ctrl => QueueMsg::SelectAll,
            "Space" | "Insert" if plain => {
                window.toggle_and_step();
                return Vec::new();
            }
            "F10" if shift => return self.open_queue_menu(None),
            "ContextMenu" if plain => return self.open_queue_menu(None),
            "p" if plain => match selected {
                Some(job) if job.state == JobState::Paused => QueueMsg::Resume(job.id),
                Some(job) if !job.state.is_finished() => QueueMsg::Pause(job.id),
                _ => return Vec::new(),
            },
            "Delete" if plain => QueueMsg::Remove,
            "Enter" if plain => match selected {
                Some(job) if job.question.is_some() => QueueMsg::Answer(job.id),
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        self.on_queue(message)
    }

    /// A message of the queue window.
    pub(super) fn on_queue(&mut self, message: QueueMsg) -> Vec<Effect> {
        // A scroll changes which rows are built, nothing else: an open menu stays.
        if let QueueMsg::Scrolled(offset) = message {
            if let Some(window) = self.queue_window.as_mut() {
                window.scrolled = offset;
            }
            return Vec::new();
        }
        if !matches!(message, QueueMsg::Menu(_) | QueueMsg::Focused(..)) {
            // Whatever else is done in the window closes its menu.
            if let Some(window) = self.queue_window.as_mut() {
                window.menu = None;
            }
        }
        match message {
            QueueMsg::Focused(id, focused) => {
                self.dialog_windows.focus(id, focused);
                return Vec::new();
            }
            QueueMsg::Own(own) => {
                self.dialog_windows.queue_own = own;
                return Vec::new();
            }
            QueueMsg::Select(job) => {
                let held = self.modifiers_held();
                if let Some(window) = self.queue_window.as_mut() {
                    window.click(job, held.ctrl, held.shift);
                }
                return Vec::new();
            }
            QueueMsg::RowMenu(job) => return self.open_queue_menu(Some(job)),
            QueueMsg::Menu(message) => return self.on_queue_menu(message),
            QueueMsg::Remove => return self.queue_remove(),
            QueueMsg::Confirm => return self.queue_confirm(),
            _ => {}
        }
        let Some(window) = self.queue_window.as_mut() else {
            return Vec::new();
        };
        let command = match message {
            QueueMsg::Move(step) => {
                let jobs = window.chosen();
                if jobs.is_empty() {
                    return Vec::new();
                }
                Command::QueueMoveMany { jobs, step }
            }
            QueueMsg::Pause(job) => Command::OpsPause { job },
            QueueMsg::Resume(job) => Command::OpsResume { job },
            QueueMsg::Keep => {
                window.confirm = None;
                return Vec::new();
            }
            QueueMsg::SelectAll => {
                window.select_all();
                return Vec::new();
            }
            QueueMsg::PauseAll => Command::QueuePauseAll {},
            QueueMsg::ResumeAll => Command::QueueResumeAll {},
            QueueMsg::ClearFinished => Command::QueueClearFinished {},
            QueueMsg::Answer(job) => {
                self.file_ops.jobs.ask_first(job);
                // The question is the main window's: in a window of its own, it takes the keys.
                return self.main_window_front();
            }
            QueueMsg::Close => {
                self.queue_window = None;
                return Vec::new();
            }
            QueueMsg::Lists(message) => return self.on_list(message),
            QueueMsg::Focused(..)
            | QueueMsg::Own(_)
            | QueueMsg::Select(_)
            | QueueMsg::RowMenu(_)
            | QueueMsg::Menu(_)
            | QueueMsg::Remove
            | QueueMsg::Confirm
            | QueueMsg::Scrolled(_) => return Vec::new(),
        };
        vec![Effect::Dispatch(self.active, vec![command])]
    }
}

#[cfg(test)]
#[path = "queue_window_tests.rs"]
mod tests;
