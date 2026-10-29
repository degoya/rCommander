//! The `settings` tool group: thin wrappers over `settings.get` and `settings.set`, and
//! `keymap.list`, which reads the keys in force. Moving a key (`keymap.set`) has no tool: it
//! changes what one key press of a person runs, so only the person does it (RC-100-21).
//!
//! The engine refuses every protected key (`mcp.*`, `vault.*`) in `settings.set` for every
//! caller, so no call here can switch the server, change its port, its token or its policy, or
//! the vault's idle lock: an agent never loosens its own permissions. The token is stored in
//! the settings table (`mcp.token`, `token.rs`) but is no row of the schema, so `settings_get`
//! never lists it and it is never in an answer.

use rc_engine::Command;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::Value;

use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct GetParams {
    /// Only this setting, such as `appearance.theme`; every one when omitted.
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct SetParams {
    /// The setting, as settings_get names it.
    pub key: String,
    /// The new value, of the setting's type: true/false, a whole number, a choice or a text.
    pub value: Value,
}

#[tool_router(router = settings_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read rCommander's settings: each with setting (its key, as key and settings_set take it), value (the default when nothing valid is stored), default, type (bool, int, choice, id, path, or font: a font family name, empty for the default, a missing one draws in the default), min and max or choices, protected (only a person changes it in the window) and restart (takes effect at the next start). Pass key for one setting. Error: setting_unknown."
    )]
    pub(crate) async fn settings_get(
        &self,
        Parameters(params): Parameters<GetParams>,
    ) -> ToolResult {
        self.run(Command::GetSettings { key: params.key }).await
    }

    #[tool(
        description = "Read rCommander's keys in force: one row per key with keys (such as Ctrl+K; empty for a row whose keys the person took away), name (the action, as the command palette names it), when (files: while the panel shows its file list; always: in a terminal too), command (the registry command with its params; none for a key the window handles itself), changed (the person's keymap differs from the standard here) and locked (focus, escape, access_key or window: a key the window keeps); stale lists stored deviations this version no longer has. Read only: only a person changes keys, in the settings."
    )]
    pub(crate) async fn keymap_list(&self) -> ToolResult {
        self.run(Command::ListKeymap).await
    }

    #[tool(
        description = "Change one setting, such as appearance.theme to system, light or dark; it takes effect at once unless restart is true. The MCP and vault settings (server switch, port, token, policy, idle lock) are refused with setting_protected: only a person changes them in rCommander. Answers the setting with its new value. Errors: setting_unknown, setting_protected, setting_invalid."
    )]
    pub(crate) async fn settings_set(
        &self,
        Parameters(params): Parameters<SetParams>,
    ) -> ToolResult {
        self.run(Command::SetSetting {
            key: params.key,
            value: params.value,
        })
        .await
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
