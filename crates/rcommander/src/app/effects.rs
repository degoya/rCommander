//! What the window does with an [`Effect`]: commands queued for the engine, tasks, the dialogs'
//! jobs; a click's commands; storing the tabs and the geometry before the app ends.

use iced::Task;
use rc_engine::{Command, CursorTarget, Engine, PanelId};

use super::{
    App, Effect, Job, Message, database_dialog::DatabaseMsg, field_walk, file_ops, queue_window,
    window_state,
};

impl App {
    /// A click on `row` of `panel`, or a double-click with `open`: the panel becomes active, the
    /// cursor goes to the row, and a double-click opens it as Enter does - in this order, as one
    /// job. A double-click on `..` goes up.
    pub(super) fn pointing(&mut self, panel: PanelId, row: usize, open: bool) -> Effect {
        let mut commands = Vec::with_capacity(3);
        if panel != self.active {
            commands.push(Command::SwitchActivePanel { panel: Some(panel) });
        }
        let pane = self.pane_mut(panel);
        let parent_row = pane.entry_index(row).is_none();
        if let Some(index) = pane.point_at(row) {
            commands.push(Command::SetCursor {
                panel: Some(panel),
                target: CursorTarget::Index(index),
            });
        }
        if open {
            commands.push(if parent_row {
                Command::Navigate {
                    panel: Some(panel),
                    path: "..".into(),
                }
            } else {
                Command::OpenFile {
                    panel: Some(panel),
                    path: None,
                }
            });
        }
        Effect::Dispatch(panel, commands)
    }

    pub(super) fn perform(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::Dispatch(panel, commands) => {
                // The receiver lives as long as the window's subscriptions: a send fails only
                // while the app ends, when nothing is left to run anyway.
                let _ = self.jobs.send(Job { panel, commands });
                Task::none()
            }
            Effect::ShowTerminal(panel) => self.show_terminal(panel),
            Effect::FocusTerminal(panel) => self.focus_terminal(panel),
            Effect::Task(task) => task,
            Effect::WalkFields { back, then } => match then {
                Some(then) => field_walk::walk(back).chain(field_walk::focused().map(then)),
                None => field_walk::walk(back),
            },
            Effect::Database(job) => Task::perform(job.run(self.engine.clone()), |result| {
                Message::Database(DatabaseMsg::Finished(Box::new(result)))
            }),
            Effect::HostKey(job) => Task::perform(job.run(self.engine.clone()), Message::HostKey),
            Effect::Ops(submit) => Task::perform(submit.run(self.engine.clone()), |done| {
                Message::FileOps(file_ops::FileOpsMsg::Submitted(Box::new(done)))
            }),
            Effect::Properties(job) => self.properties_task(job),
            Effect::Connections(job) => {
                Task::perform(job.run(self.engine.clone()), Message::Connections)
            }
            Effect::CertTrust(job) => {
                Task::perform(job.run(self.engine.clone()), Message::CertTrust)
            }
            Effect::MultiRename(call) => {
                Task::perform(call.run(self.engine.clone()), Message::MultiRename)
            }
            Effect::QueueList(call) => {
                Task::perform(call.run(self.engine.clone()), queue_window::answered)
            }
            Effect::TrustStore(call) => {
                Task::perform(call.run(self.engine.clone()), std::convert::identity)
            }
            Effect::Plugins(call) => {
                Task::perform(call.run(self.engine.clone()), std::convert::identity)
            }
        }
    }

    /// Stores the panels' tabs, the window's geometry and that of an open dialog window, then
    /// ends the app.
    pub(super) fn save_and_exit(&self) -> Task<Message> {
        let Some(db) = self.db.clone() else {
            return super::startup::leave();
        };
        let geometry = self.window.geometry();
        let dialogs = self.dialog_windows.open_geometries();
        let engine = self.engine.clone();
        Task::perform(
            async move {
                // Tabs and places that cannot be stored are only missing at the next start.
                if let Err(error) = engine.save_tabs().await {
                    tracing::warn!(%error, "the panels' tabs were not stored");
                }
                for (dialog, at) in dialogs {
                    if let Err(error) = db.set(dialog.key(), &at).await {
                        tracing::warn!(
                            key = dialog.key(),
                            %error,
                            "a dialog window's place was not stored"
                        );
                    }
                }
                if let Err(error) = window_state::save(&db, geometry).await {
                    tracing::warn!(
                        key = window_state::GEOMETRY_KEY,
                        %error,
                        "the window's place was not stored"
                    );
                }
            },
            |_| Message::Saved,
        )
    }
}

/// Runs a window job: [`Message::Done`] with the code of its first failure, or
/// [`Message::VaultRefused`] when a locked vault refused a command - with that command and those
/// after it, which the unlock prompt runs again once the master password opened the vault.
pub async fn run_job(engine: &Engine, job: Job) -> Message {
    let mut commands = job.commands.into_iter();
    while let Some(command) = commands.next() {
        // Kept for a refusal; a command is a few words.
        let again = command.clone();
        match engine.dispatch(command).await {
            Ok(_) => {}
            Err(error) if error.code() == "vault_locked" => {
                let retry = std::iter::once(again).chain(commands).collect();
                return Message::VaultRefused(job.panel, retry);
            }
            Err(error) => return Message::Done(job.panel, Err(error.code())),
        }
    }
    Message::Done(job.panel, Ok(()))
}

/// Runs `commands` one after the other on `engine`, stopping at the first failure; its code.
pub async fn run_in_order(engine: &Engine, commands: Vec<Command>) -> Result<(), &'static str> {
    for command in commands {
        engine
            .dispatch(command)
            .await
            .map_err(|error| error.code())?;
    }
    Ok(())
}
