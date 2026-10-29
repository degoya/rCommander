//! The queue window's actions on several jobs (RC-010-15, owner 2026-10-18): the selection taken
//! off (`queue.discard`) - after one question when one of its jobs has started, which is then
//! cancelled as a single cancel does it -, the shown list or every list cleared (`queue.clear`)
//! after a question naming the jobs that have not finished, and the context menu opened, walked
//! and taken (`menu.rs`).

use rc_engine::{Chord, Command, JobId};

use super::{
    QueueMsg,
    lists::{ListMsg, NameAction},
    menu::{MenuAction, MenuKey, MenuMsg, QueueMenu},
    selection::started,
};
use crate::{
    app::{App, Effect},
    i18n::t,
};

/// A question before jobs leave the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    /// The selected jobs taken off; `started` of them are cancelled.
    Remove { jobs: Vec<JobId>, started: usize },
    /// The shown list cleared, or every list; `unfinished` of their jobs have not finished.
    Clear { all_lists: bool, unfinished: usize },
}

impl Confirm {
    /// The question, its yes and its no.
    pub fn texts(&self) -> (String, &'static str, &'static str) {
        match self {
            // One running job: the question a single cancel always asked.
            Self::Remove { jobs, started: 1 } if jobs.len() == 1 => (
                t!("queue.confirm_cancel").to_owned(),
                t!("queue.yes"),
                t!("queue.no"),
            ),
            Self::Remove { jobs, started } => (
                t!(
                    "queue.confirm_remove",
                    count = jobs.len(),
                    started = started
                ),
                t!("queue.remove_yes"),
                t!("queue.keep"),
            ),
            Self::Clear {
                all_lists: false,
                unfinished,
            } => (
                t!("queue.confirm_clear", unfinished = unfinished),
                t!("queue.clear_yes"),
                t!("queue.keep"),
            ),
            Self::Clear {
                all_lists: true,
                unfinished,
            } => (
                t!("queue.confirm_clear_all", unfinished = unfinished),
                t!("queue.clear_yes"),
                t!("queue.keep"),
            ),
        }
    }
}

fn dispatch(app: &App, command: Command) -> Vec<Effect> {
    vec![Effect::Dispatch(app.active, vec![command])]
}

impl App {
    /// Delete, Remove: the selected jobs leave the queue at once while none has started, else
    /// after the question.
    pub(super) fn queue_remove(&mut self) -> Vec<Effect> {
        let Some(window) = self.queue_window.as_mut() else {
            return Vec::new();
        };
        let chosen = window.chosen_entries();
        let jobs: Vec<JobId> = chosen.iter().map(|entry| entry.job.id).collect();
        let begun = chosen.iter().filter(|entry| started(entry)).count();
        if jobs.is_empty() {
            return Vec::new();
        }
        if begun > 0 {
            window.confirm = Some(Confirm::Remove {
                jobs,
                started: begun,
            });
            return Vec::new();
        }
        window.unmark();
        dispatch(self, Command::QueueDiscard { jobs })
    }

    /// Clear this list, Clear all lists: at once while every job has finished, else after the
    /// question naming how many have not.
    pub(super) fn queue_clear(&mut self, all_lists: bool) -> Vec<Effect> {
        let Some(window) = self.queue_window.as_mut() else {
            return Vec::new();
        };
        let unfinished = if all_lists {
            window.lists.iter().map(|list| list.jobs).sum::<usize>()
        } else {
            // The jobs run at once shown with the default list are in none: clearing leaves them.
            let open =
                |entry: &&rc_engine::QueueEntry| !entry.at_once && !entry.job.state.is_finished();
            window.entries.iter().filter(open).count()
        };
        if unfinished > 0 {
            window.confirm = Some(Confirm::Clear {
                all_lists,
                unfinished,
            });
            return Vec::new();
        }
        let list = window.list;
        window.unmark();
        dispatch(self, Command::QueueClear { list, all_lists })
    }

    /// Yes to the open question.
    pub(super) fn queue_confirm(&mut self) -> Vec<Effect> {
        let Some(window) = self.queue_window.as_mut() else {
            return Vec::new();
        };
        let command = match window.confirm.take() {
            Some(Confirm::Remove { jobs, .. }) => Command::QueueDiscard { jobs },
            Some(Confirm::Clear { all_lists, .. }) => Command::QueueClear {
                list: window.list,
                all_lists,
            },
            None => return Vec::new(),
        };
        window.unmark();
        dispatch(self, command)
    }

    /// The menu over the job list, for the selection; a right click on a job that is not among
    /// the selected selects it alone first.
    pub(super) fn open_queue_menu(&mut self, on: Option<JobId>) -> Vec<Effect> {
        let Some(window) = self.queue_window.as_mut() else {
            return Vec::new();
        };
        if let Some(job) = on.filter(|job| !window.chosen().contains(job)) {
            window.click(job, false, false);
        }
        window.menu = Some(QueueMenu::new(window));
        Vec::new()
    }

    /// A click on the menu or the pointer on it.
    pub(super) fn on_queue_menu(&mut self, message: MenuMsg) -> Vec<Effect> {
        let Some(menu) = self.queue_window.as_mut().and_then(|w| w.menu.as_mut()) else {
            return Vec::new();
        };
        let taken = match message {
            MenuMsg::Pick(index) => menu.take(index),
            MenuMsg::Hover(index) => {
                menu.hover(index);
                MenuKey::Stay
            }
            MenuMsg::Close => MenuKey::Close,
        };
        self.after_menu(taken)
    }

    /// A key while the menu is open.
    pub(super) fn queue_menu_key(&mut self, chord: &Chord) -> Vec<Effect> {
        let Some(menu) = self.queue_window.as_mut().and_then(|w| w.menu.as_mut()) else {
            return Vec::new();
        };
        let taken = menu.key(chord);
        self.after_menu(taken)
    }

    fn after_menu(&mut self, taken: MenuKey) -> Vec<Effect> {
        let action = match taken {
            MenuKey::Stay => return Vec::new(),
            MenuKey::Close => None,
            MenuKey::Take(action) => Some(action),
        };
        if let Some(window) = self.queue_window.as_mut() {
            window.menu = None;
        }
        match action {
            Some(MenuAction::Move(step)) => self.on_queue(QueueMsg::Move(step)),
            Some(MenuAction::Assign(list)) => self.on_list(ListMsg::Assign(list)),
            Some(MenuAction::Gather) => self.on_list(ListMsg::Name(NameAction::Gather)),
            Some(MenuAction::Remove) => self.queue_remove(),
            Some(MenuAction::SelectAll) => self.on_queue(QueueMsg::SelectAll),
            Some(MenuAction::Clear { all_lists }) => self.queue_clear(all_lists),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "bulk_tests.rs"]
mod tests;
