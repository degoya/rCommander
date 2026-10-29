//! `panel_filter`: Total Commander's quick filter (Ctrl+S) for agents - a panel shows only the
//! entries whose names match, until the filter is lifted or the panel moves to another folder.

use rc_engine::Command;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use super::panels::PanelArg;
use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FilterParams {
    /// With `*` or `?` the whole name must match (`*.txt`); a text without them matches any
    /// part of the name. Case is ignored. Omitted or empty: every entry shows again. At most 256
    /// characters.
    #[serde(default)]
    pub pattern: Option<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[tool_router(router = panel_filter_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Filter a panel as Ctrl+S does: only entries whose names match pattern show (*.txt and t?do* match whole names, a text without wildcards any part, ignoring case); omit pattern to show every entry again. Selected entries the filter hides are deselected, so no operation acts on what is not shown. The filter holds until lifted or the panel goes to another folder. Answers the panel's state with its filter and entry_count. Errors: filter_invalid."
    )]
    pub(crate) async fn panel_filter(
        &self,
        Parameters(params): Parameters<FilterParams>,
    ) -> ToolResult {
        self.run(Command::Filter {
            panel: params.panel.map(Into::into),
            pattern: params.pattern,
            last: false,
        })
        .await
    }
}

#[cfg(test)]
#[path = "panel_filter_tests.rs"]
mod tests;
