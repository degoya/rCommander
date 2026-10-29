//! The `login_scripts` tool group: thin wrappers over the `login_scripts.*` commands.
//!
//! A script reads and writes as steps; a step that sends a secret names where it is stored
//! (`{"kind": "connection_password"}` or `{"kind": "stored", "id": 3}`), never its value, and a
//! script lists its own secrets by id and label only. Storing or removing a secret is the
//! person's, in rCommander (`login_scripts.set_secret`, `login_scripts.clear_secret` are
//! `gui_only`); a call whose arguments name a secret is refused like the connection tools'
//! (`secret_not_accepted`). Making a connection run a script runs it on a server, so
//! `login_scripts_assign` is an exec tool, and so is a create or update that brings steps
//! (`Command::effective_class`, RC-080-42): each asks for a confirmation code. A `send_secret`
//! step, the steps before it and assigning a script with one are the window's only (`gui_only`,
//! refused by the engine).

use rc_engine::{
    Command, Connection, ConnectionId, EngineError, LoginScriptDeletion, LoginScriptDraft,
    LoginScriptId, LoginScriptInfo, LoginScriptPatch, Outcome, Pattern, ScriptInvalid,
    ScriptSecretRef, Step,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::{
    RcMcpServer,
    answer::{ToolResult, respond},
};

/// One step, as tools write it.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "step", rename_all = "snake_case")]
pub(crate) enum StepArg {
    /// Wait until the output since the last match (colour codes removed) matches `pattern`, a
    /// regular expression of at most 512 characters; `timeout_ms` defaults to the script's.
    WaitFor {
        pattern: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    /// Send `text` (at most 1024 characters), and Enter after it when `newline`.
    SendText {
        text: String,
        #[serde(default)]
        newline: bool,
    },
    /// Send a stored secret, and Enter after it unless `newline` is false.
    SendSecret {
        source: SourceArg,
        #[serde(default = "enter_by_default")]
        newline: bool,
    },
    /// Send Enter alone.
    SendLine,
    /// Wait `ms` milliseconds, at most 60000.
    Sleep { ms: u64 },
}

fn enter_by_default() -> bool {
    true
}

/// Where a `send_secret` step's secret is stored.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SourceArg {
    /// The password stored with the connection the script runs on.
    ConnectionPassword,
    /// A secret the person stored with this script, by its id from login_scripts_get.
    Stored { id: i64 },
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct IdParams {
    /// The script's id, from login_scripts_list.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct CreateParams {
    /// Unique among the scripts, ignoring case.
    pub name: String,
    /// At most 64 steps, run in order.
    #[serde(default)]
    pub steps: Vec<StepArg>,
    /// What a wait_for without its own timeout waits; 15000 when omitted, at most 600000.
    #[serde(default)]
    pub step_timeout_ms: Option<u64>,
    /// How long a whole run may take; 120000 when omitted, at most 600000.
    #[serde(default)]
    pub total_timeout_ms: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct UpdateParams {
    /// The script's id.
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    /// Replaces every step when given.
    #[serde(default)]
    pub steps: Option<Vec<StepArg>>,
    #[serde(default)]
    pub step_timeout_ms: Option<u64>,
    #[serde(default)]
    pub total_timeout_ms: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct DeleteParams {
    /// The script's id.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct AssignParams {
    /// The connection's id, from connections_list.
    pub connection_id: i64,
    /// The script to run after the login; none when omitted.
    #[serde(default)]
    pub script_id: Option<i64>,
}

/// The steps as the engine takes them; a pattern that does not compile names its step.
impl CreateParams {
    /// The `login_scripts.create` command; `Err` for a step that does not read.
    pub(crate) fn command(self) -> Result<Command, EngineError> {
        steps(self.steps).map(|steps| Command::CreateLoginScript {
            script: LoginScriptDraft {
                name: self.name,
                steps,
                step_timeout_ms: self.step_timeout_ms,
                total_timeout_ms: self.total_timeout_ms,
            },
        })
    }
}

impl UpdateParams {
    /// The `login_scripts.update` command; `Err` for a step that does not read.
    pub(crate) fn command(self) -> Result<Command, EngineError> {
        self.steps
            .map(steps)
            .transpose()
            .map(|steps| Command::UpdateLoginScript {
                id: LoginScriptId(self.id),
                patch: LoginScriptPatch {
                    name: self.name,
                    steps,
                    step_timeout_ms: self.step_timeout_ms,
                    total_timeout_ms: self.total_timeout_ms,
                },
            })
    }
}

fn steps(args: Vec<StepArg>) -> Result<Vec<Step>, EngineError> {
    args.into_iter()
        .enumerate()
        .map(|(index, arg)| -> Result<Step, EngineError> {
            Ok(match arg {
                StepArg::WaitFor {
                    pattern,
                    timeout_ms,
                } => Step::WaitFor {
                    pattern: Pattern::new(&pattern).map_err(|error| ScriptInvalid::Pattern {
                        step: Some(index),
                        error,
                    })?,
                    timeout_ms,
                },
                StepArg::SendText { text, newline } => Step::SendText { text, newline },
                StepArg::SendSecret { source, newline } => Step::SendSecret {
                    source: match source {
                        SourceArg::ConnectionPassword => ScriptSecretRef::ConnectionPassword,
                        SourceArg::Stored { id } => ScriptSecretRef::Stored { id },
                    },
                    newline,
                },
                StepArg::SendLine => Step::SendLine,
                StepArg::Sleep { ms } => Step::Sleep { ms },
            })
        })
        .collect()
}

/// An outcome as these tools answer it.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    Scripts {
        scripts: Vec<LoginScriptInfo>,
    },
    Script(LoginScriptInfo),
    Deleted {
        deleted: LoginScriptId,
        #[serde(flatten)]
        deletion: LoginScriptDeletion,
    },
    /// The connection a script was assigned to, as the `connections_*` tools answer it.
    Connection(Connection),
    Other(Outcome),
}

fn view(outcome: Outcome) -> View {
    match outcome {
        Outcome::LoginScripts { scripts } => View::Scripts { scripts },
        Outcome::LoginScript { script } => View::Script(script),
        Outcome::Connection { connection } => View::Connection(connection),
        Outcome::LoginScriptDeleted { id, deletion } => View::Deleted {
            deleted: id,
            deletion,
        },
        other => View::Other(other),
    }
}

impl RcMcpServer {
    async fn run_login_scripts(&self, command: Result<Command, EngineError>) -> ToolResult {
        match command {
            Ok(command) => self.dispatch_as(command, view).await,
            Err(error) => respond::<View>(Err(error)),
        }
    }
}

#[tool_router(router = login_scripts_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Every login script, by name: {scripts: [{id, name, steps, step_timeout_ms, total_timeout_ms, secrets: [{id, label}], connections: [connection ids that run it]}]}. A login script is what rCommander types after an SSH login: steps wait_for (a regex on the output), send_text, send_secret (a stored secret by reference, never its value), send_line (Enter) and sleep."
    )]
    pub(crate) async fn login_scripts_list(&self) -> ToolResult {
        self.run_login_scripts(Ok(Command::ListLoginScripts)).await
    }

    #[tool(
        description = "One login script by id: its steps, timeouts, its stored secrets by id and label (never a value) and the connections that run it. Errors: login_script_not_found."
    )]
    pub(crate) async fn login_scripts_get(
        &self,
        Parameters(params): Parameters<IdParams>,
    ) -> ToolResult {
        self.run_login_scripts(Ok(Command::GetLoginScript {
            id: LoginScriptId(params.id),
        }))
        .await
    }

    #[tool(
        description = "Add a login script. Steps: {step: wait_for, pattern, timeout_ms?}, {step: send_text, text, newline?}, {step: send_secret, source: {kind: connection_password} or {kind: stored, id}, newline? (true)}, {step: send_line}, {step: sleep, ms}. Takes no secrets: a password among the arguments is refused (secret_not_accepted); the person stores a script's secrets in rCommander. With steps it is exec: the first call answers with a question and a confirmation code; ask the person, and only if they agree call again with confirmation set to the code. A script with a send_secret step is made by the person in rCommander (gui_only). Errors: gui_only, name_empty, name_taken, login_script_pattern_invalid, login_script_too_many_steps, login_script_text_too_long, login_script_timeout_invalid."
    )]
    pub(crate) async fn login_scripts_create(
        &self,
        Parameters(params): Parameters<CreateParams>,
    ) -> ToolResult {
        self.run_login_scripts(params.command()).await
    }

    #[tool(
        description = "Change a login script's name, timeouts or steps (given steps replace all of them; the step shapes are login_scripts_create's). Takes no secrets (secret_not_accepted). With steps it is exec and needs a confirmation code like login_scripts_assign; the steps up to the last send_secret step stay as they are and no send_secret step is added (gui_only), steps after it may change. Answers the changed script. Errors: gui_only, login_script_not_found, name_empty, name_taken, login_script_pattern_invalid, login_script_too_many_steps, login_script_text_too_long, login_script_timeout_invalid."
    )]
    pub(crate) async fn login_scripts_update(
        &self,
        Parameters(params): Parameters<UpdateParams>,
    ) -> ToolResult {
        self.run_login_scripts(params.command()).await
    }

    #[tool(
        description = "Delete a login script with its stored secrets; the connections that ran it run none. Destructive: the first call answers with a question and a confirmation code; ask the person, and only if they agree call again with confirmation set to the code. Answers {deleted, secrets, connections}. Errors: login_script_not_found."
    )]
    pub(crate) async fn login_scripts_delete(
        &self,
        Parameters(params): Parameters<DeleteParams>,
    ) -> ToolResult {
        self.run_login_scripts(Ok(Command::DeleteLoginScript {
            id: LoginScriptId(params.id),
        }))
        .await
    }

    #[tool(
        description = "Set the login script a connection runs after its SSH login, or none without script_id. Exec: the script will type into the server, so the first call answers with a question and a confirmation code; ask the person, and only if they agree call again with confirmation set to the code. A script with a send_secret step is assigned by the person in rCommander (gui_only). Answers the connection. Errors: gui_only, connection_not_found, login_script_not_found."
    )]
    pub(crate) async fn login_scripts_assign(
        &self,
        Parameters(params): Parameters<AssignParams>,
    ) -> ToolResult {
        self.run_login_scripts(Ok(Command::AssignLoginScript {
            connection_id: ConnectionId(params.connection_id),
            script_id: params.script_id.map(LoginScriptId),
        }))
        .await
    }
}

#[cfg(test)]
#[path = "login_scripts_tests.rs"]
mod tests;
