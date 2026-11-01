//! The window's work for the engine (RC-100-68), off the window's thread: one command whose
//! answer comes back tagged, and the list read again. Every step is a command of the registry
//! (`workspaces.*`); the window keeps nothing the engine does not have.
//!
//! An answer is read as the MCP reads it - its fields by name (`{ workspaces }`, `{ id }`) - so
//! the window depends on the commands' answers, not on how the engine names them inside.

use iced::Task;
use rc_engine::{Command, Engine, Outcome, Workspace, WorkspaceId, workspaces::ON_START_KEY};
use serde::de::DeserializeOwned;

use super::{WsMsg, known};
use crate::app::Message;

/// What an answer is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// A new workspace of the panels as they are (`workspaces.capture` without an id): the
    /// cursor goes to it.
    Created,
    /// The form saved (`workspaces.update`); applied then, and the window closed, when `apply`.
    Saved { apply: bool },
    /// The panels as they are taken into the workspace at the cursor: its form is read again.
    Captured,
    /// A rename, a move or a delete: the next list shows it.
    Changed,
    /// `workspaces.apply`: what it left out goes to the status bar, a failure to the active
    /// panel's status line - the window may be closed by then.
    Applied,
}

/// `workspaces.list`, its answer as [`WsMsg::Listed`]; the start workspace
/// (`workspaces.on_start`) is read with it for the form's switch.
pub fn list(engine: Engine) -> Task<Message> {
    Task::perform(
        async move {
            match engine.dispatch(Command::ListWorkspaces {}).await {
                Ok(outcome) => {
                    let settings = Command::GetSettings {
                        key: Some(ON_START_KEY.to_owned()),
                    };
                    let start = engine.dispatch(settings).await.ok();
                    known::set_start(start.as_ref().and_then(start_of));
                    field::<Vec<Workspace>>(&outcome, "workspaces").ok_or("database_error")
                }
                Err(error) => Err(error.code()),
            }
        },
        |listed| Message::Workspaces(WsMsg::Listed(listed)),
    )
}

/// The workspace `workspaces.on_start` names in a `settings.get` answer; `None` when it is
/// empty or not a number.
pub fn start_of(outcome: &Outcome) -> Option<WorkspaceId> {
    let Outcome::Settings { settings } = outcome else {
        return None;
    };
    settings
        .iter()
        .find(|setting| setting.key == ON_START_KEY)
        .and_then(|setting| setting.value.as_str())
        .and_then(|value| value.trim().parse().ok())
}

/// `command`, its answer as [`WsMsg::Answer`] for `then`.
pub fn run(engine: Engine, then: Then, command: Command) -> Task<Message> {
    Task::perform(
        async move { engine.dispatch(command).await.map_err(|error| error.code()) },
        move |answer| Message::Workspaces(WsMsg::Answer(then, Box::new(answer))),
    )
}

/// The field `name` of `outcome` as the MCP sees it; `None` when it has none of that shape.
pub fn field<T: DeserializeOwned>(outcome: &Outcome, name: &str) -> Option<T> {
    let value = serde_json::to_value(outcome).ok()?;
    serde_json::from_value(value.get(name)?.clone()).ok()
}

#[cfg(test)]
#[path = "calls_tests.rs"]
mod tests;
