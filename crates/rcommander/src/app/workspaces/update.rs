//! The window's messages (RC-100-68): the list's keys and buttons, the name of a new or renamed
//! workspace, the delete question, the form, and the engine's answers - each change a command
//! (`calls.rs`), the list read again after it.

use rc_engine::{Command, Outcome, WorkspaceId, workspaces::ON_START_KEY};

use super::{
    NameFor, Pending, WsMsg, calls,
    calls::Then,
    focus,
    form::{self, FormMsg},
    keys::{self, Focus},
    known, list,
    panel_edit::PanelMsg,
    view::{LIST_STOP_ID, NAME_FIELD},
};
use crate::{
    app::{
        App, Effect, Message,
        palette::workspaces::key_name,
        settings::{Page, SettingsMessage, ShortcutsMsg, Tab, WorkspaceKeyMsg},
    },
    i18n::{error_text, t},
};

impl App {
    /// A message of the window.
    pub(in crate::app) fn on_workspaces(&mut self, message: WsMsg) -> Vec<Effect> {
        match message {
            WsMsg::SaveCurrentAs => {
                let mut effects = self.open_workspaces();
                effects.extend(self.on_workspaces(WsMsg::New));
                return effects;
            }
            WsMsg::Listed(Ok(list)) => return self.workspaces_listed(list),
            WsMsg::Listed(Err(code)) => {
                tracing::warn!(code, "the workspaces were not read");
                if let Some(window) = self.workspaces.as_mut() {
                    window.note = Some((error_text(code), true));
                }
                return Vec::new();
            }
            WsMsg::Answer(then, answer) => return self.workspaces_answer(then, *answer),
            WsMsg::Close => {
                self.workspaces = None;
                return Vec::new();
            }
            WsMsg::AddKey => return self.workspace_key(),
            WsMsg::StartUp(on) => return self.workspace_start_up(on),
            WsMsg::Apply => return self.apply_workspace(),
            WsMsg::Activate(id) => {
                if let Some(window) = self.workspaces.as_mut() {
                    window.select(Some(id), &known::all());
                }
                return self.apply_workspace();
            }
            WsMsg::Form(form) => return self.workspace_form(form),
            _ => {}
        }
        let Some(window) = self.workspaces.as_mut() else {
            return Vec::new();
        };
        let engine = self.engine.clone();
        let run = |then, command| vec![Effect::Task(calls::run(engine, then, command))];
        let rows = known::rows();
        match message {
            WsMsg::Filter(filter) => {
                window.filter = filter;
                let shown = window.shown(&rows);
                let cursor = list::keep(&shown, window.cursor, None);
                if cursor != window.cursor {
                    window.select(cursor, &known::all());
                }
            }
            WsMsg::Select(id) => {
                window.select(Some(id), &known::all());
                window.focus = Focus::List;
            }
            WsMsg::Step(by) => {
                let cursor = list::step(&window.shown(&rows), window.cursor, by);
                if cursor != window.cursor {
                    window.select(cursor, &known::all());
                }
            }
            WsMsg::New => {
                let text = list::new_name(&rows, t!("workspaces.new_name"));
                window.pending = Some(Pending::Name {
                    target: NameFor::New,
                    text,
                });
                return vec![focus(NAME_FIELD)];
            }
            WsMsg::Rename => {
                let named = window
                    .cursor
                    .and_then(|id| rows.iter().find(|row| row.id == id));
                if let Some(row) = named {
                    window.pending = Some(Pending::Name {
                        target: NameFor::Rename(row.id),
                        text: row.name.clone(),
                    });
                    return vec![focus(NAME_FIELD)];
                }
            }
            WsMsg::Delete => window.pending = window.cursor.map(Pending::Delete),
            WsMsg::NameInput(typed) => {
                if let Some(Pending::Name { text, .. }) = window.pending.as_mut() {
                    *text = typed;
                }
            }
            WsMsg::NameSubmit => {
                let Some(Pending::Name { target, text }) = window.pending.clone() else {
                    return Vec::new();
                };
                let name = text.trim().to_owned();
                if !form::name_fits(&name) {
                    window.note = Some((error_text("workspace_name"), true));
                    return Vec::new();
                }
                window.pending = None;
                window.focus = Focus::List;
                let command = match target {
                    NameFor::New => Command::CaptureWorkspace {
                        id: None,
                        name: Some(name),
                    },
                    NameFor::Rename(id) => Command::RenameWorkspace { id, name },
                };
                let then = match target {
                    NameFor::New => Then::Created,
                    NameFor::Rename(_) => Then::Changed,
                };
                let mut effects = run(then, command);
                effects.push(focus(LIST_STOP_ID));
                return effects;
            }
            WsMsg::Cancel => {
                window.pending = None;
                window.focus = Focus::List;
                return vec![focus(LIST_STOP_ID)];
            }
            WsMsg::Confirm(yes) => {
                let asked = window.pending.take();
                if let (true, Some(Pending::Delete(id))) = (yes, asked) {
                    return run(Then::Changed, Command::DeleteWorkspace { id });
                }
            }
            WsMsg::Move(up) => {
                let place = window
                    .cursor
                    .and_then(|id| list::moved_to(&rows, id, up).map(|position| (id, position)));
                if let Some((id, position)) = place {
                    return run(Then::Changed, Command::MoveWorkspace { id, position });
                }
            }
            WsMsg::Capture => {
                if let Some(id) = window.cursor {
                    let command = Command::CaptureWorkspace {
                        id: Some(id),
                        name: None,
                    };
                    return run(Then::Captured, command);
                }
            }
            WsMsg::Save => {
                let Some(form) = window.form.as_ref() else {
                    return Vec::new();
                };
                match form.request() {
                    Ok(request) => {
                        window.busy = true;
                        let command = Command::UpdateWorkspace(request);
                        return run(Then::Saved { apply: false }, command);
                    }
                    Err(code) => window.note = Some((error_text(code), true)),
                }
            }
            WsMsg::Focused(id) => window.focus = keys::focus_of(id.as_ref()),
            WsMsg::SaveCurrentAs
            | WsMsg::Close
            | WsMsg::Listed(_)
            | WsMsg::Answer(..)
            | WsMsg::Activate(_)
            | WsMsg::Form(_)
            | WsMsg::Apply
            | WsMsg::AddKey
            | WsMsg::StartUp(_) => {}
        }
        Vec::new()
    }

    /// The form's start switch: the workspace at the cursor becomes `workspaces.on_start`, or the
    /// setting is emptied and the start shows the last run's tabs alone.
    fn workspace_start_up(&mut self, on: bool) -> Vec<Effect> {
        let Some(id) = self.workspaces.as_ref().and_then(|window| window.cursor) else {
            return Vec::new();
        };
        known::set_start(on.then_some(id));
        let value = if on { id.to_string() } else { String::new() };
        let set = Command::SetSetting {
            key: ON_START_KEY.to_owned(),
            value: serde_json::Value::from(value),
        };
        vec![Effect::Dispatch(self.active, vec![set])]
    }

    /// Enter, a double click or Apply: the workspace at the cursor applied - its changes saved
    /// first - and the window closed. A failure shows in the active panel's status line, as a
    /// key's does.
    fn apply_workspace(&mut self) -> Vec<Effect> {
        let Some(window) = self.workspaces.as_mut() else {
            return Vec::new();
        };
        let Some(id) = window.cursor else {
            return Vec::new();
        };
        if let Some(form) = window
            .form
            .as_ref()
            .filter(|form| form.dirty && form.id == id)
        {
            return match form.request() {
                Ok(request) => {
                    window.busy = true;
                    let command = Command::UpdateWorkspace(request);
                    let then = Then::Saved { apply: true };
                    vec![Effect::Task(calls::run(self.engine.clone(), then, command))]
                }
                Err(code) => {
                    window.note = Some((error_text(code), true));
                    Vec::new()
                }
            };
        }
        self.workspaces = None;
        self.apply_workspace_now(id)
    }

    /// `workspaces.apply` of `id`, from the window, the palette, the menu, a workspace's key or
    /// the start (`workspaces.on_start`); its answer comes back to the window's books.
    pub(in crate::app) fn apply_workspace_now(&self, id: WorkspaceId) -> Vec<Effect> {
        let command = Command::ApplyWorkspace { id };
        vec![Effect::Task(calls::run(
            self.engine.clone(),
            Then::Applied,
            command,
        ))]
    }

    /// The answer to `workspaces.apply`: a failure in the active panel's status line, as a key's;
    /// what it left out - a tab whose server is gone, a terminal that did not start - in the
    /// status bar, each as `<code>:<what>`.
    fn workspace_applied(&mut self, answer: Result<Outcome, &'static str>) -> Vec<Effect> {
        let outcome = match answer {
            Ok(outcome) => outcome,
            Err(code) => {
                let active = self.active;
                self.pane_mut(active).error = Some(error_text(code));
                return Vec::new();
            }
        };
        let skipped: Vec<String> = calls::field(&outcome, "skipped").unwrap_or_default();
        if !skipped.is_empty() {
            let notes: Vec<String> = skipped.iter().map(|note| skipped_note(note)).collect();
            self.info = Some(t!("workspaces.skipped", notes = notes.join("; ")));
        }
        Vec::new()
    }

    /// A change of the form; a tab that takes the panel's folder gets it here.
    fn workspace_form(&mut self, message: FormMsg) -> Vec<Effect> {
        let message = match message {
            FormMsg::Panel(panel, PanelMsg::FromPanel(tab)) => FormMsg::Panel(
                panel,
                PanelMsg::Location(tab, self.engine.panel(panel).path),
            ),
            FormMsg::Panel(panel, PanelMsg::Add) => {
                FormMsg::Panel(panel, PanelMsg::AddTab(self.engine.panel(panel).path))
            }
            message => message,
        };
        if let Some(form) = self
            .workspaces
            .as_mut()
            .and_then(|window| window.form.as_mut())
        {
            form.update(message);
        }
        Vec::new()
    }

    /// The answer to a command of the window.
    fn workspaces_answer(
        &mut self,
        then: Then,
        answer: Result<Outcome, &'static str>,
    ) -> Vec<Effect> {
        if then == Then::Applied {
            return self.workspace_applied(answer);
        }
        let Some(window) = self.workspaces.as_mut() else {
            return Vec::new();
        };
        window.busy = false;
        let outcome = match answer {
            Ok(outcome) => outcome,
            Err(code) => {
                window.note = Some((error_text(code), true));
                return Vec::new();
            }
        };
        window.note = None;
        match then {
            Then::Created => {
                let id = calls::field::<WorkspaceId>(&outcome, "id");
                window.filter.clear();
                window.select(id, &known::all());
            }
            Then::Saved { apply } => {
                if let Some(form) = window.form.as_mut() {
                    form.saved();
                }
                if apply {
                    return self.apply_workspace();
                }
                window.note = Some((t!("workspaces.saved").to_owned(), false));
            }
            Then::Captured => {
                if let Some(form) = window.form.as_mut() {
                    form.dirty = false;
                }
                window.note = Some((t!("workspaces.captured").to_owned(), false));
            }
            Then::Changed | Then::Applied => {}
        }
        // The engine announces the change as well; this read comes after the answer for sure.
        vec![Effect::Task(self.workspaces_task())]
    }

    /// "Add key...": the window closes and the Shortcuts tab opens with this workspace chosen for
    /// its key - the settings draw over every dialog.
    fn workspace_key(&mut self) -> Vec<Effect> {
        let Some(id) = self
            .workspaces
            .as_ref()
            .and_then(|window| window.form.as_ref())
            .map(|form| form.id)
        else {
            return Vec::new();
        };
        self.workspaces = None;
        let mut effects = self.apply(Message::Settings(SettingsMessage::OpenPage(Page::Keyboard)));
        effects.extend(self.apply(Message::Settings(SettingsMessage::Tab(Tab::Shortcuts))));
        let open = WorkspaceKeyMsg::OpenFor(key_name(id));
        effects.extend(self.apply(Message::Settings(SettingsMessage::Shortcuts(
            ShortcutsMsg::Workspace(open),
        ))));
        effects
    }
}

/// A note of `workspaces.apply`, `<code>:<what>`, in words: the code's message and what it was
/// about; a note without a code as it is.
fn skipped_note(note: &str) -> String {
    match note.split_once(':') {
        Some((code, what))
            if !code.is_empty() && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') =>
        {
            format!("{} ({what})", error_text(code))
        }
        _ => note.to_owned(),
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
