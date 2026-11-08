//! The `user_commands` tool group (RC-100-86): thin wrappers over `usercmd.list`,
//! `start_menu.list`, `usercmd.run` and `start_menu.run`. An own command `em_<name>` is a program
//! with Total Commander's placeholders, internal rows or another own command; the Start menu is a
//! tree of such entries. Which program runs is a person's choice: saving, deleting and moving them
//! is the window's (`omitted` in the coverage table), and the run tools take no parameters of the
//! caller's - a call with them, a command that starts as administrator or asks for its parameters
//! (`?`) is the person's (`GuiOnly`, `rc_engine::ESCALATIONS`).

use rc_engine::Command;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use super::panels::PanelArg;
use crate::{RcMcpServer, answer::ToolResult};

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct RunParams {
    /// The own command's name as user_commands_list names it (em_..., case does not count).
    pub name: String,
    /// The panel whose folder, cursor and marked names fill the placeholders: `left` or
    /// `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

impl RunParams {
    /// The command this call dispatches: the stored parameters, never the caller's.
    pub(crate) fn command(&self) -> Command {
        Command::RunUserCommand {
            name: self.name.clone(),
            panel: self.panel.map(Into::into),
            params: None,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct StartRunParams {
    /// The Start menu entry's id as start_menu_list names it.
    pub id: i64,
    /// The panel whose folder, cursor and marked names fill the placeholders: `left` or
    /// `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

impl StartRunParams {
    /// The command this call dispatches: the stored parameters, never the caller's.
    pub(crate) fn command(&self) -> Command {
        Command::RunStartItem {
            id: self.id,
            panel: self.panel.map(Into::into),
            params: None,
        }
    }
}

#[tool_router(router = user_commands_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read the own commands (Total Commander's em_ commands) by name: each with id, name (em_..., lower case), tooltip, icon_path (a file, or rc-symbol:<name> for a symbol of rCommander's own collection; saving checks the name) and icon_index, and action - kind program (command the program, params with Total Commander's placeholders such as %P%N, %S or %UL, a leading ? asks the person for them first; folder, show normal, minimized or maximized; elevated starts it as administrator), internal (command: rows separated by commas, keymap names or Total Commander's cm_ names) or own (command: another em_ name)."
    )]
    pub(crate) async fn user_commands_list(&self) -> ToolResult {
        self.run(Command::ListUserCommands {}).await
    }

    #[tool(
        description = "Read the Start menu in menu order, each submenu before its rows: each row with id, parent (null at the top), depth, kind item, submenu or separator, title and, for an item, its action as user_commands_list describes it."
    )]
    pub(crate) async fn start_menu_list(&self) -> ToolResult {
        self.run(Command::ListStartMenu {}).await
    }

    #[tool(
        description = "Run an own command for a panel with its stored parameters: a program started with the panel's folder, cursor and marked names in its placeholders (never through a shell), its internal rows one after the other, or the own command it names. One that starts as administrator or asks for its parameters (?) is the person's to run. Answers started, the programs started. Errors: user_command_unknown, user_command_name, user_command_unknown_row, user_command_window_only, user_command_recursion, user_command_unsafe_argument, user_command_list_encoding, user_command_local_only, user_command_not_found, user_command_failed."
    )]
    pub(crate) async fn user_commands_run(
        &self,
        Parameters(params): Parameters<RunParams>,
    ) -> ToolResult {
        self.run(params.command()).await
    }

    #[tool(
        description = "Run an entry of the Start menu for a panel, as user_commands_run runs an own command; a submenu or a separator does not run. One that starts as administrator or asks for its parameters (?) is the person's to run. Errors: start_menu_unknown, start_menu_invalid, and those of user_commands_run."
    )]
    pub(crate) async fn start_menu_run(
        &self,
        Parameters(params): Parameters<StartRunParams>,
    ) -> ToolResult {
        self.run(params.command()).await
    }
}

#[cfg(test)]
#[path = "user_commands_tests.rs"]
mod tests;
