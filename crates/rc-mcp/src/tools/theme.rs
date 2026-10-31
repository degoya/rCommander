//! The `theme` tool group (RC-100-50): thin wrappers over `theme.list` and `theme.select`. A theme
//! changes what the person sees, never a file. Saving, editing, importing and exporting a theme
//! have no tool: the editor with its live preview and the file dialog are the window's
//! (`theme.save`, `theme.delete` are `GuiOnly`).

use rc_engine::{Command, settings::themes::ThemeMode};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{RcMcpServer, answer::ToolResult};

/// The mode a theme is for.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Light,
    Dark,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct SelectParams {
    /// The mode whose theme changes: light or dark.
    pub mode: Mode,
    /// The theme's id, as theme_list names it (such as nord-dark).
    pub id: String,
}

#[tool_router(router = theme_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read rCommander's colour themes: each with id, name, mode (light or dark), builtin (shipped with rCommander, not changeable) and selected (the theme that mode draws with). appearance.theme (system, light, dark) decides which mode is in force; theme_select changes a mode's theme."
    )]
    pub(crate) async fn theme_list(&self) -> ToolResult {
        self.run(Command::ListThemes {}).await
    }

    #[tool(
        description = "Choose the colour theme of the light or the dark mode by its id from theme_list; the window draws with it at once, open terminals included. The theme must be of that mode. Answers the themes as theme_list does. Errors: theme_unknown, theme_wrong_mode."
    )]
    pub(crate) async fn theme_select(
        &self,
        Parameters(params): Parameters<SelectParams>,
    ) -> ToolResult {
        let mode = match params.mode {
            Mode::Light => ThemeMode::Light,
            Mode::Dark => ThemeMode::Dark,
        };
        self.run(Command::SelectTheme {
            mode,
            id: params.id,
        })
        .await
    }
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
