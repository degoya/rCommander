//! The queue window's lists (RC-010-15): which list it shows, a new list, a list renamed,
//! deleted, started or stopped, saved and loaded again, a saved list deleted after a yes, the
//! selected jobs put into another list or a new one (`queue.assign_many`, `queue.create` with
//! jobs). Every action is a registry command (`queue.*`) run as a [`ListCall`],
//! whose answer comes back to the window: the saved lists, what a save left out or a load could
//! not load, or the code of a refusal.

use std::fmt;

use iced::widget::{Id, operation};
use rc_engine::{Command, DEFAULT_LIST, Engine, JobId, Outcome, QueueList, QueueListId, SavedList};

use super::{QueueMsg, QueueWindow};
use crate::{
    app::{App, Effect, Message},
    i18n::t,
};

/// The id of the name field.
pub(super) const NAME_ID: &str = "rcommander.queue.name";

/// A list as a choice of a pick list: its name, the default list's from the catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListChoice {
    pub id: QueueListId,
    label: String,
}

impl fmt::Display for ListChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// The lists as choices, in their order.
pub fn list_choices(lists: &[QueueList]) -> Vec<ListChoice> {
    lists
        .iter()
        .map(|list| ListChoice {
            id: list.id,
            label: list_name(list),
        })
        .collect()
}

/// A list's name as shown: the default list is "Queue".
pub fn list_name(list: &QueueList) -> String {
    if list.id == DEFAULT_LIST {
        t!("queue.list.default").to_owned()
    } else {
        list.name.clone()
    }
}

/// A saved list as a choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedChoice {
    pub id: i64,
    label: String,
}

impl fmt::Display for SavedChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

pub fn saved_choices(saved: &[SavedList]) -> Vec<SavedChoice> {
    saved
        .iter()
        .map(|saved| SavedChoice {
            id: saved.id,
            label: t!(
                "queue.saved_entry",
                name = saved.name,
                count = saved.entries
            ),
        })
        .collect()
}

/// What the name field is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameAction {
    Create,
    Rename,
    Save,
    /// A new list the selected jobs go into.
    Gather,
}

/// The open name field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Naming {
    pub action: NameAction,
    pub text: String,
    /// For a save: load the list again when the app starts.
    pub restore_on_start: bool,
    /// For a new list of the selection: the jobs selected when the name was asked for.
    pub jobs: Vec<JobId>,
}

/// What the last list action did, for the line under the lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// A saved list, and how many jobs were left out of it.
    Saved { name: String, left_out: usize },
    /// A loaded list, and how many of its entries could not be loaded.
    Loaded { jobs: usize, unloadable: usize },
    /// A refusal, by its code.
    Refused(&'static str),
}

#[derive(Debug, Clone)]
pub enum ListMsg {
    /// The list the window shows.
    Show(QueueListId),
    /// New list, Rename, Save: the name field opens.
    Name(NameAction),
    NameInput(String),
    RestoreOnStart(bool),
    /// Enter in the name field.
    NameSubmit,
    NameCancel,
    Delete,
    /// Start (`true`) or stop the shown list.
    Start(bool),
    ChooseSaved(i64),
    Load,
    /// Delete the chosen saved list: asks first.
    DeleteSaved,
    ConfirmDeleteSaved,
    KeepSaved,
    /// The selected jobs into another list.
    Assign(QueueListId),
    /// A list command's answer.
    /// Boxed: an answer carries the saved lists and is far larger than every other message.
    Answered(Box<ListAnswer>),
}

/// A list command for the engine; its answer comes back to the window.
#[derive(Debug)]
pub struct ListCall {
    command: Command,
}

/// What became of a [`ListCall`]: the outcome, or the code of the refusal.
pub type ListAnswer = Result<Outcome, &'static str>;

impl ListCall {
    pub async fn run(self, engine: Engine) -> ListAnswer {
        engine
            .dispatch(self.command)
            .await
            .map_err(|error| error.code())
    }
}

/// The message a [`ListCall`]'s answer comes back as.
pub fn answered(answer: ListAnswer) -> Message {
    Message::Queue(QueueMsg::Lists(ListMsg::Answered(Box::new(answer))))
}

fn call(command: Command) -> Vec<Effect> {
    vec![Effect::QueueList(ListCall { command })]
}

impl App {
    /// The saved lists read again for the open window.
    pub(super) fn read_saved_lists(&self) -> Vec<Effect> {
        if self.queue_window.is_none() {
            return Vec::new();
        }
        call(Command::QueueLists {})
    }

    /// A message of the lists.
    pub(super) fn on_list(&mut self, message: ListMsg) -> Vec<Effect> {
        let Some(window) = self.queue_window.as_mut() else {
            return Vec::new();
        };
        let list = window.list;
        let command = match message {
            ListMsg::Show(shown) => {
                window.list = shown;
                window.selected = None;
                window.unmark();
                self.refresh_queue_window();
                return Vec::new();
            }
            ListMsg::Name(action) => {
                let text = match action {
                    NameAction::Create | NameAction::Gather => String::new(),
                    NameAction::Rename | NameAction::Save => window
                        .lists
                        .iter()
                        .find(|found| found.id == list)
                        .map(|found| found.name.clone())
                        .unwrap_or_default(),
                };
                let jobs = if action == NameAction::Gather {
                    window.chosen()
                } else {
                    Vec::new()
                };
                window.naming = Some(Naming {
                    action,
                    text,
                    restore_on_start: false,
                    jobs,
                });
                let id = Id::new(NAME_ID);
                return vec![Effect::Task(iced::Task::batch([
                    operation::focus(id.clone()),
                    operation::select_all(id),
                ]))];
            }
            ListMsg::NameInput(text) => {
                if let Some(naming) = &mut window.naming {
                    naming.text = text;
                }
                return Vec::new();
            }
            ListMsg::RestoreOnStart(on) => {
                if let Some(naming) = &mut window.naming {
                    naming.restore_on_start = on;
                }
                return Vec::new();
            }
            ListMsg::NameSubmit => {
                let Some(naming) = window.naming.take() else {
                    return Vec::new();
                };
                let name = naming.text;
                match naming.action {
                    NameAction::Create | NameAction::Gather => Command::QueueCreate {
                        name,
                        jobs: naming.jobs,
                    },
                    NameAction::Rename => Command::QueueRename { list, name },
                    NameAction::Save => Command::QueueSave {
                        list,
                        name: Some(name),
                        restore_on_start: naming.restore_on_start,
                    },
                }
            }
            ListMsg::NameCancel => {
                window.naming = None;
                return Vec::new();
            }
            ListMsg::Delete => Command::QueueDelete { list },
            ListMsg::Start(true) => Command::QueueStart { list },
            ListMsg::Start(false) => Command::QueuePause { list },
            ListMsg::ChooseSaved(saved) => {
                window.saved_choice = Some(saved);
                return Vec::new();
            }
            ListMsg::Load => match window.saved_choice {
                Some(saved) => Command::QueueLoad { saved },
                None => return Vec::new(),
            },
            ListMsg::DeleteSaved => {
                window.confirm_delete_saved = window.saved_choice;
                return Vec::new();
            }
            ListMsg::ConfirmDeleteSaved => match window.confirm_delete_saved.take() {
                Some(saved) => Command::QueueDeleteSaved { saved },
                None => return Vec::new(),
            },
            ListMsg::KeepSaved => {
                window.confirm_delete_saved = None;
                return Vec::new();
            }
            ListMsg::Assign(into) => {
                let jobs = window.chosen();
                if jobs.is_empty() {
                    return Vec::new();
                }
                Command::QueueAssignMany { jobs, list: into }
            }
            ListMsg::Answered(answer) => {
                window.take_answer(*answer);
                self.refresh_queue_window();
                return Vec::new();
            }
        };
        window.note = None;
        call(command)
    }
}

impl QueueWindow {
    /// A list command's answer: the saved lists kept, what a save or a load did noted, a refusal
    /// shown; a loaded list is the one shown next.
    fn take_answer(&mut self, answer: ListAnswer) {
        match answer {
            Ok(Outcome::QueueLists { saved, .. }) => {
                if !saved
                    .iter()
                    .any(|found| Some(found.id) == self.saved_choice)
                {
                    self.saved_choice = saved.first().map(|found| found.id);
                }
                self.saved = saved;
            }
            Ok(Outcome::QueueSaved { saved, left_out }) => {
                self.saved_choice = Some(saved.id);
                self.note = Some(Note::Saved {
                    name: saved.name,
                    left_out: left_out.len(),
                });
            }
            Ok(Outcome::QueueLoaded {
                list,
                jobs,
                unloadable,
            }) => {
                self.list = list.id;
                self.selected = None;
                self.note = Some(Note::Loaded {
                    jobs: jobs.len(),
                    unloadable: unloadable.len(),
                });
            }
            Ok(_) => {}
            Err(code) => self.note = Some(Note::Refused(code)),
        }
    }
}

#[cfg(test)]
#[path = "lists_tests.rs"]
mod tests;
