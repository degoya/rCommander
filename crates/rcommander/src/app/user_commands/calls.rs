//! The window's work for the engine (RC-100-86), off the window's thread: the two lists read, and
//! one command whose answer comes back tagged. Every step is a command of the registry
//! (`usercmd.*`, `start_menu.*`); the window keeps nothing the engine does not have.

use iced::Task;
use rc_engine::{Command, Engine, Outcome, StartItem, UserCommand, UserCommandsOutcome};

use super::{UcMsg, list::Place};
use crate::app::Message;

/// What an answer is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// An own command saved: the cursor goes to it.
    OwnSaved,
    /// A row of the Start menu saved: the cursor goes to it; a new one moves to `place` then.
    StartSaved { new: Option<Place> },
    /// A delete or a move: the next list shows it.
    Changed,
}

/// `usercmd.list`, its answer as [`UcMsg::Commands`].
pub fn list_commands(engine: Engine) -> Task<Message> {
    Task::perform(
        async move {
            match engine.dispatch(Command::ListUserCommands {}).await {
                Ok(Outcome::UserCommands(outcome)) => match *outcome {
                    UserCommandsOutcome::Commands { commands } => Ok(commands),
                    _ => Err("database_error"),
                },
                Ok(_) => Err("database_error"),
                Err(error) => Err(error.code()),
            }
        },
        |listed: Result<Vec<UserCommand>, &'static str>| {
            Message::UserCommands(UcMsg::Commands(listed))
        },
    )
}

/// `start_menu.list`, its answer as [`UcMsg::StartMenu`].
pub fn list_start_menu(engine: Engine) -> Task<Message> {
    Task::perform(
        async move {
            match engine.dispatch(Command::ListStartMenu {}).await {
                Ok(Outcome::UserCommands(outcome)) => match *outcome {
                    UserCommandsOutcome::StartMenu { items } => Ok(items),
                    _ => Err("database_error"),
                },
                Ok(_) => Err("database_error"),
                Err(error) => Err(error.code()),
            }
        },
        |listed: Result<Vec<StartItem>, &'static str>| {
            Message::UserCommands(UcMsg::StartMenu(listed))
        },
    )
}

/// `command`, its answer as [`UcMsg::Answer`] for `then`.
pub fn run(engine: Engine, then: Then, command: Command) -> Task<Message> {
    Task::perform(
        async move { engine.dispatch(command).await.map_err(|error| error.code()) },
        move |answer| Message::UserCommands(UcMsg::Answer(then, Box::new(answer))),
    )
}

/// The id a save answered with; `None` for any other answer.
pub fn saved_id(outcome: &Outcome) -> Option<i64> {
    match outcome {
        Outcome::UserCommands(outcome) => match **outcome {
            UserCommandsOutcome::Saved { id } => Some(id),
            _ => None,
        },
        _ => None,
    }
}
