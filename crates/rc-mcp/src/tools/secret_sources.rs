//! The secret sources (RC-100-34; the Windows Credential Manager, RC-100-37): which are offered
//! and whether each answers, and a search in one - entry ids, titles, users and addresses, never
//! a value. The tools are named `credential_sources_*`: no tool name says `secret`
//! (`vault_tests.rs`, `ssh_import_tests.rs`).
//!
//! There is deliberately no tool that maps or unmaps a connection's secret to a source: those
//! `secrets.source_map` and `secrets.source_unmap` commands are `CommandClass::GuiOnly`, omitted
//! in `coverage.rs` and refused by `Engine::dispatch_mcp` - where a secret comes from is the
//! person's decision. A value a source hands out for a connect is scrubbed from every answer.

use rc_engine::{Command, ConnectionId};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SourcesParams {
    /// Also this connection's references: which secret comes from which source and entry.
    #[serde(default)]
    pub connection_id: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SearchParams {
    /// The source as credential_sources_list names it (`plugin:<id>` or `wincred`).
    pub source: String,
    /// What to look for in the entries' titles, users and addresses.
    pub query: String,
}

#[tool_router(router = secret_sources_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the secret sources (password-manager plugins, and on Windows the Credential Manager) a connection's password, private key or passphrase can come from instead of the vault: source (plugin:<id> or wincred), name, state (ready, locked, unreachable, unlicensed, or failed with the plugin's code). With connection_id: that connection's references too (kind, source, entry_id, entry_label) - never a value. Which secret comes from which source is set only by the person in rCommander. Errors: connection_not_found."
    )]
    pub(crate) async fn credential_sources_list(
        &self,
        Parameters(params): Parameters<SourcesParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::ListSecretSources {
            connection_id: params.connection_id.map(ConnectionId),
        })
        .await
    }

    #[tool(
        description = "Search one secret source for entries: id, title, user and url of at most 50 entries, never a password, key or code. Errors: secret_source_missing (not installed or switched off), secret_source_locked (the person unlocks the password manager), secret_source_unreachable, secret_source_unlicensed, secret_source_failed, secret_source_entry_missing."
    )]
    pub(crate) async fn credential_sources_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::SearchSecretSource {
            source: params.source,
            query: params.query,
        })
        .await
    }
}

#[cfg(test)]
#[path = "secret_sources_tests.rs"]
mod tests;
