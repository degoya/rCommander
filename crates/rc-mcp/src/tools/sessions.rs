//! The SSH session tools: connect, disconnect, test, and a connection's session settings.
//!
//! No tool takes or answers a secret: the login reads the password, key and passphrase from the
//! vault inside rCommander. A locked vault answers `vault_locked` - no tool unlocks it, and no
//! call opens the unlock dialog (`Engine::dispatch_mcp`); a server key that is not trusted
//! answers `host_key_unknown` or `host_key_changed`, and only a person decides in the window.

use rc_engine::{AuthKind, Command, ConnectionId, Outcome, SessionPrefs};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::{
    RcMcpServer,
    answer::{ToolResult, respond},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct IdParams {
    /// The connection's id, from connections_list.
    pub id: i64,
}

/// An authentication method, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MethodArg {
    Agent,
    Key,
    Password,
}

impl From<MethodArg> for AuthKind {
    fn from(method: MethodArg) -> Self {
        match method {
            MethodArg::Agent => Self::Agent,
            MethodArg::Key => Self::Key,
            MethodArg::Password => Self::Password,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct PrefsParams {
    /// The connection's id.
    pub id: i64,
    /// The methods to try, in order, each once: agent, key, password.
    #[serde(default)]
    pub auth_order: Option<Vec<MethodArg>>,
    /// The SHA256: fingerprint of the one agent key to offer (ssh_agent_identities lists
    /// them); an empty string offers every key.
    #[serde(default)]
    pub agent_fingerprint: Option<String>,
    /// Seconds of silence before a keepalive, 1 to 3600.
    #[serde(default)]
    pub keepalive_secs: Option<u32>,
    /// Seconds without a channel before the session closes, 0 (never) to 86400.
    #[serde(default)]
    pub idle_close_secs: Option<u32>,
}

/// An outcome as these tools answer it: without the engine's `kind` tag.
#[derive(Serialize)]
#[serde(untagged)]
enum View {
    Session(rc_engine::SessionStatus),
    Tested(rc_engine::SessionTestReport),
    Prefs {
        connection_id: ConnectionId,
        #[serde(flatten)]
        prefs: SessionPrefs,
    },
    Other(Box<Outcome>),
}

fn view(outcome: Outcome) -> View {
    match outcome {
        Outcome::Session(status) => View::Session(status),
        Outcome::SessionTested(report) => View::Tested(report),
        Outcome::SessionPrefs {
            connection_id,
            prefs,
        } => View::Prefs {
            connection_id,
            prefs,
        },
        other => View::Other(Box::new(other)),
    }
}

impl RcMcpServer {
    async fn run_session(&self, command: Command) -> ToolResult {
        self.dispatch_as(command, view).await
    }
}

#[tool_router(router = sessions_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Open the SSH session of a connection, or take the one that is up. The server's host key is checked first, then the login tries the connection's methods in order (default: agent, key, password; secrets come from rCommander's vault, never through MCP). Answers connection_id, state, login {method: agent|key|password|keyboard_interactive, fingerprint for a key}, host_key_fingerprint, channels. Errors: connection_not_found, connection_not_ssh, vault_locked (ask the person to unlock the vault in rCommander), host_key_unknown and host_key_changed (only the person can trust or replace a key, in the window), ssh_auth_failed, ssh_unreachable, ssh_timeout, ssh_no_username, key_passphrase_wrong, cancelled."
    )]
    pub(crate) async fn connections_connect(
        &self,
        Parameters(params): Parameters<IdParams>,
    ) -> ToolResult {
        self.run_session(Command::ConnectSession {
            id: ConnectionId(params.id),
        })
        .await
    }

    #[tool(
        description = "Close the session of a connection - SSH, or an FTP connection's kept session (every control connection says QUIT) - or cancel a setup under way (within a second). Channels on it end with connection_lost. A panel showing the server goes back to the local folder it showed before, as Ctrl+Shift+F does. Answers was_open: whether there was a session or a setup. Errors: connection_not_found."
    )]
    pub(crate) async fn connections_disconnect(
        &self,
        Parameters(params): Parameters<IdParams>,
    ) -> ToolResult {
        self.run_session(Command::DisconnectSession {
            id: ConnectionId(params.id),
        })
        .await
    }

    #[tool(
        description = "Test a connection without keeping a session: connect, check the host key, log in, disconnect. Answers login (the method that got in), host_key {algorithm, fingerprint, public_key} and duration_ms. Errors as connections_connect."
    )]
    pub(crate) async fn connections_test(
        &self,
        Parameters(params): Parameters<IdParams>,
    ) -> ToolResult {
        self.run_session(Command::TestConnection {
            id: ConnectionId(params.id),
        })
        .await
    }

    #[tool(
        description = "A connection's SSH session settings: auth_order (agent, key, password in the order tried), agent_fingerprint (the one agent key offered, or null for every key), keepalive_secs (30 by default) and idle_close_secs (600 by default, 0 never). Errors: connection_not_found."
    )]
    pub(crate) async fn connections_session_prefs(
        &self,
        Parameters(params): Parameters<IdParams>,
    ) -> ToolResult {
        self.run_session(Command::GetSessionPrefs {
            id: ConnectionId(params.id),
        })
        .await
    }

    #[tool(
        description = "Change a connection's SSH session settings; omitted fields stay. Used by the connection's next session. Answers the settings. Errors: connection_not_found, session_prefs_invalid (an empty or repeated auth_order, keepalive_secs outside 1 to 3600, idle_close_secs above 86400, an agent_fingerprint that is not SHA256:...)."
    )]
    pub(crate) async fn connections_set_session_prefs(
        &self,
        Parameters(params): Parameters<PrefsParams>,
    ) -> ToolResult {
        let id = ConnectionId(params.id);
        let engine = &self.context.engine;
        let current = match engine.dispatch_mcp(Command::GetSessionPrefs { id }).await {
            Ok(Outcome::SessionPrefs { prefs, .. }) => prefs,
            Ok(other) => return respond(Ok(view(other))),
            Err(error) => return respond::<View>(Err(error)),
        };
        let prefs = SessionPrefs {
            auth_order: params.auth_order.map_or(current.auth_order, |order| {
                order.into_iter().map(AuthKind::from).collect()
            }),
            agent_fingerprint: params
                .agent_fingerprint
                .map_or(current.agent_fingerprint, Some),
            keepalive_secs: params.keepalive_secs.unwrap_or(current.keepalive_secs),
            idle_close_secs: params.idle_close_secs.unwrap_or(current.idle_close_secs),
        };
        self.run_session(Command::SetSessionPrefs { id, prefs })
            .await
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
