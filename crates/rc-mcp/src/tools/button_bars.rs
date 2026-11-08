//! The `button_bars` tool group (RC-100-87): thin wrappers over `button_bar.list`,
//! `button_bar.get` and `button_bar.run`. A button bar holds buttons that run a command of the
//! own commands' model (a program with Total Commander's placeholders, internal rows, an own
//! command `em_`) or open another bar as a sub-bar. Which program a button runs is a person's
//! choice: saving, renaming, deleting and importing bars is the window's (`omitted` in the
//! coverage table), and the run tool takes neither parameters nor files of the caller's - a call
//! with them, or a button that starts as administrator or asks for its parameters (`?`), is the
//! person's (`GuiOnly`, `rc_engine::ESCALATIONS`).

use rc_engine::Command;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use super::panels::PanelArg;
use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct GetParams {
    /// The bar's id as button_bars_list names it.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct RunParams {
    /// The bar's id as button_bars_list names it.
    pub bar: i64,
    /// The button's place among the bar's items, from 0 (separators count).
    pub button: usize,
    /// The panel whose folder, cursor and marked names fill the placeholders: `left` or
    /// `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

impl RunParams {
    /// The command this call dispatches: the stored parameters, no files, never the caller's.
    pub(crate) fn command(&self) -> Command {
        Command::RunButton {
            bar: self.bar,
            button: self.button,
            panel: self.panel.map(Into::into),
            params: None,
            files: Vec::new(),
        }
    }
}

#[tool_router(router = button_bars_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read the button bars by id: each with id, name and spec - version and items, each an item of type separator or button with action (command: an own command's action as user_commands_list describes it - kind program, internal or own; or sub_bar: the id of the bar it opens as a menu), icon (path - a file, or rc-symbol:<name> for a symbol of rCommander's own collection - and index) and tooltip. The settings buttonbar.horizontal and buttonbar.vertical name the bar each place shows (0 the first); appearance.button_bar and appearance.vertical_button_bar show them."
    )]
    pub(crate) async fn button_bars_list(&self) -> ToolResult {
        self.run(Command::ListButtonBars {}).await
    }

    #[tool(
        description = "Read one button bar by id, as button_bars_list describes it. Errors: button_bar_unknown."
    )]
    pub(crate) async fn button_bars_get(
        &self,
        Parameters(params): Parameters<GetParams>,
    ) -> ToolResult {
        self.run(Command::GetButtonBar { id: params.id }).await
    }

    #[tool(
        description = "Run a button of a bar for a panel with what the person stored, as user_commands_run runs an own command: a program started with the panel's folder, cursor and marked names in its placeholders (never through a shell), its internal rows, or the own command it names. A separator or a sub-bar's button runs nothing. One that starts as administrator or asks for its parameters (?) is the person's to run. Answers started, the programs started. Errors: button_bar_unknown, button_bar_no_button, button_bar_sub_bar, and those of user_commands_run."
    )]
    pub(crate) async fn button_bars_run(
        &self,
        Parameters(params): Parameters<RunParams>,
    ) -> ToolResult {
        self.run(params.command()).await
    }
}

#[cfg(test)]
#[path = "button_bars_tests.rs"]
mod tests;
