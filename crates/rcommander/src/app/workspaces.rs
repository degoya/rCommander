//! The workspaces window (RC-100-68): Ctrl+Shift+W (`workspaces.manager`), the menu bar's
//! Workspaces menu and the command palette open it. On the left the workspaces with a filter -
//! the keyboard starts there -, the active one marked; Enter or a double click applies one and
//! closes the window. On the right the form of the one at the cursor, as the connection manager
//! has it: name, own key, theme of each mode, active panel, split, and per panel its tabs, active
//! tab, view and terminal. Take current state, Apply, Save and Close below. The keys are
//! `keys.rs`, the list `list.rs`, the form `form.rs` with each panel `panel_edit.rs`, the drawing
//! `view.rs`.
//!
//! The window keeps no workspace of its own: the list is the engine's (`workspaces.list`, kept in
//! `known.rs`), read at the start and again on every `WorkspacesChanged` - also after an agent
//! changed one over MCP -, and every change is a command (`calls.rs`). A new list also brings the
//! palette's rows, the menu bar's entries and the active workspace's themes in step.

mod calls;
#[cfg(test)]
pub(crate) mod fixtures;
mod form;
mod keys;
pub mod known;
mod list;
mod panel_edit;
mod update;
mod view;

use iced::widget::{Id, operation};
use rc_engine::{Drive, Outcome, Workspace, WorkspaceId};

pub use self::view::{FILTER_FIELD, body, layer};
use self::{
    calls::Then,
    form::{Form, FormMsg},
    keys::{Focus, KeyAction},
    view::NAME_FIELD,
};
use super::{
    App, Effect, Message, field_walk,
    palette::workspaces::{WorkspaceRow, key_name},
};
use crate::theme;

/// What the window is busy with besides its list and form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    /// A name typed in the list: of a new workspace, or a new name.
    Name { target: NameFor, text: String },
    /// Delete this workspace?
    Delete(WorkspaceId),
}

/// What a typed name is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFor {
    /// A new workspace of the panels as they are.
    New,
    Rename(WorkspaceId),
}

/// The open window.
#[derive(Debug, Default)]
pub struct WorkspaceWindow {
    pub filter: String,
    /// The workspace at the cursor, whose form is shown.
    pub cursor: Option<WorkspaceId>,
    pub form: Option<Form>,
    pub focus: Focus,
    pub pending: Option<Pending>,
    /// The last outcome: a text, and whether it is an error.
    pub note: Option<(String, bool)>,
    /// A save runs.
    pub busy: bool,
}

/// What the window's controls and the engine's answers say.
#[derive(Debug, Clone)]
pub enum WsMsg {
    /// The menu's "Save current state as...": the window opens asking the new one's name.
    SaveCurrentAs,
    Close,
    /// `workspaces.list`'s answer.
    Listed(Result<Vec<Workspace>, &'static str>),
    /// The answer to a command.
    Answer(Then, Box<Result<Outcome, &'static str>>),
    Filter(String),
    /// A row clicked, or double-clicked.
    Select(WorkspaceId),
    Activate(WorkspaceId),
    /// The cursor moved by rows.
    Step(isize),
    New,
    Rename,
    Delete,
    NameInput(String),
    NameSubmit,
    /// Escape in the name field.
    Cancel,
    /// The answer to the delete question.
    Confirm(bool),
    /// Ctrl+Up (`true`) or Ctrl+Down: the workspace one place up or down.
    Move(bool),
    Form(FormMsg),
    /// Take current state: the panels as they are into the workspace at the cursor.
    Capture,
    /// Apply the workspace at the cursor - saved first when the form has changes - and close.
    Apply,
    Save,
    /// "Add key...": the Shortcuts tab with this workspace chosen.
    AddKey,
    /// "Call up when rCommander starts" switched on or off for the workspace at the cursor.
    StartUp(bool),
    /// After Tab: the id of the stop of the focus ring that has the keyboard.
    Focused(Option<Id>),
}

/// What the window draws besides its own state, read from the app when it draws.
#[derive(Debug, Clone, Default)]
pub struct Shown {
    pub rows: Vec<WorkspaceRow>,
    /// The drive bar's entries: the places, distributions and servers the form offers.
    pub drives: Vec<Drive>,
    /// The keys of the workspace at the cursor, as the palette prints them.
    pub key: Option<String>,
}

impl WorkspaceWindow {
    /// The workspaces the filter shows.
    fn shown(&self, rows: &[WorkspaceRow]) -> Vec<WorkspaceId> {
        list::shown(rows, &self.filter)
    }

    /// `list` read anew: the cursor stays on its workspace (else the active one, else the first
    /// shown), the form shows it - a draft with changes stays as typed.
    pub fn refresh(&mut self, list: &[Workspace]) {
        let rows = known::rows_of(list);
        let active = rows.iter().find(|row| row.active).map(|row| row.id);
        self.cursor = list::keep(&self.shown(&rows), self.cursor, active);
        let keep = self
            .form
            .as_ref()
            .is_some_and(|form| Some(form.id) == self.cursor && form.dirty);
        if !keep {
            self.form = self
                .cursor
                .and_then(|id| list.iter().find(|workspace| workspace.id == id))
                .map(Form::of);
        }
    }

    /// The cursor to `id`, its form read as stored.
    pub fn select(&mut self, id: Option<WorkspaceId>, list: &[Workspace]) {
        self.cursor = id;
        self.form = id
            .and_then(|id| list.iter().find(|workspace| workspace.id == id))
            .map(Form::of);
    }
}

/// The message the ring's stop after a walk turns into.
fn moved(id: Option<Id>) -> Message {
    Message::Workspaces(WsMsg::Focused(id))
}

fn focus(id: &'static str) -> Effect {
    Effect::Task(operation::focus(Id::new(id)))
}

impl App {
    /// Reads the workspaces; at the start and after every change of them.
    pub(in crate::app) fn workspaces_task(&self) -> iced::Task<Message> {
        calls::list(self.engine.clone())
    }

    /// Opens the window - or brings it to the front -, the keyboard in the filter.
    pub(in crate::app) fn open_workspaces(&mut self) -> Vec<Effect> {
        if self.workspaces.is_some() {
            let mut effects =
                self.dialog_window_front(super::dialog_windows::DialogWindow::Workspaces);
            effects.push(focus(FILTER_FIELD));
            return effects;
        }
        let mut window = WorkspaceWindow::default();
        window.refresh(&known::all());
        self.workspaces = Some(window);
        vec![focus(FILTER_FIELD), Effect::Task(self.workspaces_task())]
    }

    /// The field the window gives the keyboard when its window shows: the name being typed, else
    /// the filter.
    pub(in crate::app) fn workspaces_focus(&self) -> iced::Task<Message> {
        let naming = self
            .workspaces
            .as_ref()
            .is_some_and(|window| matches!(window.pending, Some(Pending::Name { .. })));
        operation::focus(Id::new(if naming { NAME_FIELD } else { FILTER_FIELD }))
    }

    /// What the window draws besides its own state.
    pub(in crate::app) fn workspaces_shown(&self) -> Shown {
        let key = self
            .workspaces
            .as_ref()
            .and_then(|window| window.form.as_ref())
            .and_then(|form| super::palette::menu_bar::key_of(&self.keymap, &key_name(form.id)));
        Shown {
            rows: known::rows(),
            drives: self.drive_bar.drives.clone(),
            key,
        }
    }

    /// A new list: kept for the palette and the menu, the active workspace's themes drawn, the
    /// menu bar's entries made again, the open window refreshed.
    fn workspaces_listed(&mut self, list: Vec<Workspace>) -> Vec<Effect> {
        let restyle = theme::themes::set_workspace_themes(known::active_themes(&list));
        if let Some(window) = self.workspaces.as_mut() {
            window.refresh(&list);
        }
        known::set(list);
        if restyle {
            self.restyle_terminals();
        }
        vec![Effect::Task(self.refill_menu_bar())]
    }

    /// The window's keys and clicks while it has them; `None` for anything else.
    pub(in crate::app) fn workspaces_intercept(
        &mut self,
        message: &Message,
    ) -> Option<Vec<Effect>> {
        let window = self.workspaces.as_ref()?;
        let pending = window.pending.as_ref();
        let action = match message {
            Message::Key(chord) => keys::key(window.focus, pending, chord, false),
            Message::KeyCaptured(chord) => keys::key(window.focus, pending, chord, true),
            Message::Press => return Some(Vec::new()),
            _ => return None,
        };
        Some(match action {
            KeyAction::Nothing => Vec::new(),
            KeyAction::Walk(back) => vec![Effect::WalkFields {
                back,
                then: Some(moved),
            }],
            KeyAction::Follow => vec![Effect::Task(field_walk::focused().map(moved))],
            KeyAction::Run(message) => self.on_workspaces(message),
        })
    }
}

#[cfg(test)]
#[path = "workspaces_tests.rs"]
mod tests;
