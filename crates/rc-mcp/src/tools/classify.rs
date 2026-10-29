//! The class a call really has (RC-080-42): the gate reads the call's arguments with the tool's
//! own parameter type into the command the tool dispatches, and asks the engine
//! (`Engine::effective_class`) - an overwriting copy, a login script with steps, a lowered
//! retention limit are judged on typed parameters there, never on raw JSON here.

use rc_engine::{Command, CommandClass, Engine};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::{
    archive_ops::{ExtractParams, PackParams},
    compare::DirsSyncApplyParams,
    connections_save::SaveParams,
    diagnostics::SetRetentionParams,
    login_scripts::{CreateParams, UpdateParams},
    ops::TransferParams,
    queue::SavedParams,
};

/// The tools whose calls [`command_of`] reads: every tool that covers a command of
/// `rc_engine::ESCALATIONS`; `classify_tests.rs` holds it against that table and against the
/// arms of [`command_of`].
#[cfg(test)]
pub(crate) const READ_TOOLS: &[&str] = &[
    "archives_extract",
    "archives_pack",
    "connections_save",
    "diagnostics_set_retention",
    "dirs_sync_apply",
    "login_scripts_create",
    "login_scripts_update",
    "ops_copy",
    "ops_move",
    "queue_load",
];

/// `arguments` as the parameters `T`; `None` when they do not read, and the tool then refuses
/// the call as it always did, before anything runs.
fn typed<T: DeserializeOwned>(arguments: Option<&Map<String, Value>>) -> Option<T> {
    serde_json::from_value(Value::Object(arguments.cloned().unwrap_or_default())).ok()
}

/// The command a call of `tool` dispatches, as far as its class goes (the sources a call leaves
/// to the panel stay empty); `None` for a tool not in `READ_TOOLS`.
pub(crate) fn command_of(tool: &str, arguments: Option<&Map<String, Value>>) -> Option<Command> {
    match tool {
        "ops_copy" => typed::<TransferParams>(arguments)
            .map(|params| Command::OpsCopy(params.request(params.sources.clone()))),
        "ops_move" => typed::<TransferParams>(arguments)
            .map(|params| Command::OpsMove(params.request(params.sources.clone()))),
        "archives_extract" => {
            typed::<ExtractParams>(arguments).map(|params| params.command(params.archives.clone()))
        }
        "archives_pack" => {
            typed::<PackParams>(arguments).map(|params| params.command(params.sources.clone()))
        }
        "dirs_sync_apply" => typed::<DirsSyncApplyParams>(arguments)
            .map(|params| Command::SyncApply(params.request())),
        "connections_save" => typed::<SaveParams>(arguments)
            .and_then(|params| params.request().ok())
            .map(Command::SaveConnection),
        "login_scripts_create" => {
            typed::<CreateParams>(arguments).and_then(|params| params.command().ok())
        }
        "login_scripts_update" => {
            typed::<UpdateParams>(arguments).and_then(|params| params.command().ok())
        }
        "diagnostics_set_retention" => {
            typed::<SetRetentionParams>(arguments).map(SetRetentionParams::command)
        }
        "queue_load" => typed::<SavedParams>(arguments).map(|params| Command::QueueLoad {
            saved: params.saved,
        }),
        _ => None,
    }
}

/// The class the gate holds a call of `tool` to: the engine's class of the command it
/// dispatches, never lower than `class`, the tool's own.
pub(crate) async fn effective_class(
    engine: &Engine,
    tool: &str,
    arguments: Option<&Map<String, Value>>,
    class: CommandClass,
) -> CommandClass {
    match command_of(tool, arguments) {
        Some(command) => class.stricter(engine.effective_class(&command).await),
        None => class,
    }
}

/// [`effective_class`] from the parameters alone, without what is stored: for the tool groups'
/// tests.
#[cfg(test)]
pub(crate) fn class_of(
    tool: &str,
    arguments: Option<&Map<String, Value>>,
    class: CommandClass,
) -> CommandClass {
    command_of(tool, arguments).map_or(class, |command| class.stricter(command.effective_class()))
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
