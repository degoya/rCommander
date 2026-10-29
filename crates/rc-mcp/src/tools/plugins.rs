//! The `plugins` tool group: reading the installed plugins (RC-100-09, `docs/plugins.md` "MCP").
//!
//! There is deliberately no tool that installs, previews, switches on or off, rolls back, resumes
//! or removes a plugin: those `plugins.*` commands are `CommandClass::GuiOnly`, omitted in
//! `coverage.rs` and refused by `Engine::dispatch_mcp` - they decide which foreign code runs.
//! The plugins' settings (`plugins.*`, `plugin.*`) are protected from `settings_set` as well.

use rc_engine::{Command, Outcome, PluginsOutcome};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result, json_result},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListParams {
    /// Only the plugin with this id (a UUIDv7), with its latest calls.
    #[serde(default)]
    pub id: Option<String>,
}

#[tool_router(router = plugins_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List the installed rCommander plugins: id, name, kind (viewer), the version that runs, every installed version with its state (active, previous, staged, pending, refused and why), enabled, suspended (after repeated failures), bundled, signed, permissions (claimed extensions, outputs, capabilities), signing key id and fingerprint, and the plugin's own settings. With id: that plugin and its latest calls (export, outcome, failure code, duration). plugins_enabled and development_mode are the master switch and whether unsigned packages are accepted. Plugins cannot be installed, enabled, disabled, rolled back, resumed or removed through MCP: a person decides that in the rCommander window. Errors: plugins_unavailable, plugin_not_found."
    )]
    pub(crate) async fn plugins_list(
        &self,
        Parameters(params): Parameters<ListParams>,
    ) -> ToolResult {
        match self
            .context
            .engine
            .dispatch_mcp(Command::ListPlugins { id: params.id })
            .await
        {
            Ok(Outcome::Plugins(outcome)) => match *outcome {
                PluginsOutcome::List {
                    plugins,
                    executions,
                    plugins_enabled,
                    development_mode,
                } => json_result(&serde_json::json!({
                    "plugins": plugins,
                    "executions": executions,
                    "plugins_enabled": plugins_enabled,
                    "development_mode": development_mode,
                })),
                other => unexpected(&format!("{other:?}")),
            },
            Ok(other) => unexpected(&format!("{other:?}")),
            Err(error) => Ok(error_result(error.code(), &error.to_string())),
        }
    }
}

/// An outcome this tool does not expect (cannot happen while the engine keeps its contract), as
/// an internal error rather than a guess.
fn unexpected(outcome: &str) -> ToolResult {
    tracing::warn!(outcome, "plugins.list answered something unexpected");
    Err(rmcp::ErrorData::internal_error(
        "the command answered something this tool does not expect",
        None,
    ))
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
