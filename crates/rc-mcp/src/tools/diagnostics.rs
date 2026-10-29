//! The `diagnostics` tool group: thin wrappers over the `diagnostics.*` commands - the error log
//! and the retention limits in `rcommander.db` (RC-010-11).
//!
//! The lines were masked before they were stored, and the answer passes the server's mask once
//! more on its way out. Lowering a limit is destructive (`Engine::effective_class`, RC-080-42):
//! the next sweep deletes what is over it, as clearing the log does.

use rc_engine::{Command, LogLevel};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult};

/// A stored line's level, as tools name it.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LevelArg {
    Warn,
    Error,
}

impl From<LevelArg> for LogLevel {
    fn from(level: LevelArg) -> Self {
        match level {
            LevelArg::Warn => Self::Warn,
            LevelArg::Error => Self::Error,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ErrorLogParams {
    /// Only lines of this level: `warn` or `error`.
    #[serde(default)]
    pub level: Option<LevelArg>,
    /// Only lines whose message or target contains this text, ignoring ASCII case.
    #[serde(default)]
    pub contains: Option<String>,
    /// Lines at most, newest first: 100 when omitted, 1000 at most.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct ClearErrorLogParams {}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct SetRetentionParams {
    /// The growing table, as diagnostics_get_retention names it (`error_log`).
    pub table: String,
    /// The new row limit; unchanged when omitted.
    #[serde(default)]
    pub max_rows: Option<u64>,
    /// The new age limit in days; unchanged when omitted.
    #[serde(default)]
    pub max_age_days: Option<u32>,
}

impl SetRetentionParams {
    /// The `diagnostics.set_retention` command.
    pub(crate) fn command(self) -> Command {
        Command::SetRetention(rc_engine::RetentionRequest {
            table: self.table,
            max_rows: self.max_rows,
            max_age_days: self.max_age_days,
        })
    }
}

#[tool_router(router = diagnostics_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read rCommander's error log: the stored warn and error lines (id, recorded_at UTC, level, target module, message, code), newest first. Credentials in them were masked before they were stored. Filter by level and by text in message or target; limit 100 by default, 1000 at most. info and debug lines are never stored. Error: database_unavailable."
    )]
    pub(crate) async fn diagnostics_error_log(
        &self,
        Parameters(params): Parameters<ErrorLogParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::ErrorLog(rc_engine::ErrorLogRequest {
            level: params.level.map(Into::into),
            contains: params.contains,
            limit: params.limit,
        }))
        .await
    }

    #[tool(
        description = "Delete every line of rCommander's error log and give the space back. Destructive: the first call answers with a confirmation code; call again with confirmation set to it once the person agreed. Answers how many lines were removed."
    )]
    pub(crate) async fn diagnostics_clear_error_log(
        &self,
        Parameters(_params): Parameters<ClearErrorLogParams>,
    ) -> ToolResult {
        self.dispatch_mcp(Command::ClearErrorLog).await
    }

    #[tool(
        description = "Read the retention of each table that grows with use (error_log): the limits in force (max_rows, max_age_days), the defaults, and the floor and ceiling a change must stay within. A sweep at start and every hour deletes what is over a limit, oldest first."
    )]
    pub(crate) async fn diagnostics_get_retention(&self) -> ToolResult {
        self.dispatch_mcp(Command::GetRetention).await
    }

    #[tool(
        description = "Change the row limit (max_rows) or the age limit in days (max_age_days) of a growing table; a limit left out stays. Both must lie between the table's floor and ceiling (diagnostics_get_retention): there is no unlimited. The next sweep applies it. Lowering a limit below the one in force is destructive, as the sweep then deletes lines: the person is asked, or headless the first call answers with a confirmation code. Answers every table's retention. Errors: retention_out_of_range, retention_unknown_table."
    )]
    pub(crate) async fn diagnostics_set_retention(
        &self,
        Parameters(params): Parameters<SetRetentionParams>,
    ) -> ToolResult {
        self.dispatch_mcp(params.command()).await
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
