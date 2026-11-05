//! The `hostkeys` tool group: reading the trusted SSH host keys and the revoked ones.
//!
//! There is deliberately no tool that trusts, accepts, replaces or forgets a key, and none that
//! imports a `known_hosts` file (which would trust whatever the file says): a host key is a
//! person's decision in the app (`hostkeys.answer`, `hostkeys.replace`, `hostkeys.remove` -
//! forgetting a changed key would turn the stop into a first sighting, RC-080-12 - and
//! `hostkeys.import_known_hosts` are `CommandClass::GuiOnly`, omitted in `coverage.rs` and
//! refused by `Engine::dispatch_mcp`), and none that forgets a revocation
//! (`hostkeys.remove_revoked`, RC-080-67). A connection an agent starts answers
//! `host_key_unknown` or `host_key_changed` with the fingerprint and the hint to decide in the
//! rCommander window, `host_key_revoked` for a key a `known_hosts` file revoked.

use rc_db::RevokedHostKey;
use rc_engine::{Command, KnownHost, Outcome};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::{Deserialize, Serialize};

use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result, json_result},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListParams {
    /// Only keys whose host, key type or fingerprint contains this, ignoring case; a host name
    /// also finds its hashed entries on port 22.
    #[serde(default)]
    pub filter: Option<String>,
}

/// A stored key as tools answer it: without the key blob, which only the app needs.
#[derive(Serialize)]
struct KnownHostView {
    id: i64,
    /// The host name, or `|1|salt|hash` for a hashed entry from `known_hosts`.
    host: String,
    /// 0 for a hashed entry, whose port is inside the hash.
    port: u16,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    hashed: bool,
    algorithm: String,
    fingerprint: String,
    added_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_seen_at: Option<String>,
    source: rc_db::KnownHostSource,
}

impl From<KnownHost> for KnownHostView {
    fn from(key: KnownHost) -> Self {
        Self {
            hashed: key.is_hashed(),
            id: key.id,
            host: key.host,
            port: key.port,
            algorithm: key.algorithm,
            fingerprint: key.fingerprint_sha256,
            added_at: key.added_at,
            last_seen_at: key.last_seen_at,
            source: key.source,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct RevokedParams {
    /// Only keys whose names, key type or fingerprint contain this, ignoring case.
    #[serde(default)]
    pub filter: Option<String>,
}

/// A revoked key as tools answer it: without the key blob.
#[derive(Serialize)]
struct RevokedView {
    id: i64,
    /// The `@revoked` line's names as written; the key is refused for every host.
    hosts: String,
    algorithm: String,
    fingerprint: String,
    /// The file the line came from.
    source: String,
    added_at: String,
}

impl From<RevokedHostKey> for RevokedView {
    fn from(key: RevokedHostKey) -> Self {
        Self {
            id: key.id,
            hosts: key.hosts,
            algorithm: key.algorithm,
            fingerprint: key.fingerprint_sha256,
            source: key.source,
            added_at: key.added_at,
        }
    }
}

/// An outcome these tools do not expect (cannot happen while the engine keeps its contract),
/// as an internal error rather than a guess.
fn unexpected(outcome: &Outcome) -> ToolResult {
    tracing::warn!(?outcome, "a hostkeys command answered something unexpected");
    Err(rmcp::ErrorData::internal_error(
        "the command answered something this tool does not expect",
        None,
    ))
}

#[tool_router(router = hostkeys_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the SSH host keys rCommander trusts: id, host (or |1|... for a hashed known_hosts entry, then port 0), port, key type, SHA256 fingerprint as ssh-keygen -lf prints it, added_at, last_seen_at, source (user or import). filter narrows by host, key type or fingerprint. Keys cannot be trusted or replaced through MCP: a connection to an unknown or changed key fails with host_key_unknown or host_key_changed and a person decides in the rCommander window."
    )]
    pub(crate) async fn hostkeys_list(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        match self
            .context
            .engine
            .dispatch_mcp(Command::ListHostKeys {
                filter: params.filter,
            })
            .await
        {
            Ok(Outcome::KnownHosts { keys }) => {
                let keys: Vec<KnownHostView> = keys.into_iter().map(Into::into).collect();
                json_result(&serde_json::json!({ "keys": keys }))
            }
            Ok(other) => unexpected(&other),
            Err(error) => Ok(error_result(error.code(), &error.to_string())),
        }
    }

    #[tool(
        description = "List the SSH host keys an imported known_hosts file marked @revoked: id, hosts (the line's names as written), key type, SHA256 fingerprint, source file, added_at. A server presenting one of them is refused for every host with host_key_revoked, without a question. filter narrows by names, key type or fingerprint. Only a person removes a revocation, in the rCommander window."
    )]
    pub(crate) async fn hostkeys_revoked(
        &self,
        Parameters(params): Parameters<RevokedParams>,
    ) -> ToolResult {
        match self
            .context
            .engine
            .dispatch_mcp(Command::ListRevokedHostKeys {
                filter: params.filter,
            })
            .await
        {
            Ok(Outcome::RevokedHostKeys { keys }) => {
                let keys: Vec<RevokedView> = keys.into_iter().map(Into::into).collect();
                json_result(&serde_json::json!({ "keys": keys }))
            }
            Ok(other) => unexpected(&other),
            Err(error) => Ok(error_result(error.code(), &error.to_string())),
        }
    }
}

#[cfg(test)]
#[path = "hostkeys_tests.rs"]
mod tests;
