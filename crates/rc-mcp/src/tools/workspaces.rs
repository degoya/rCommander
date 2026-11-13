//! The `workspaces` tool group (RC-100-68): thin wrappers over the `workspaces.*` commands. A
//! workspace is what both panels show, saved under a name: each panel's tabs with folder, sort,
//! view and lock, the file list or a terminal, the active panel and the divider, a theme per mode.
//! Applying one may start shells, so `workspaces_apply` has the class of `terminal_open`; its
//! question names the workspace's places (`workspace_shown.rs`), and a `spec` with a folder on
//! another machine's share is refused before it is saved (`unc.rs`, RC-110-21). The workspaces'
//! window (`workspaces.manager`, Ctrl+Shift+W) is the person's.

use rc_engine::{
    Command, CreateWorkspaceRequest, UpdateWorkspaceRequest, WorkspaceError, WorkspaceSpec,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct IdParams {
    /// The workspace's id, as workspaces_list names it.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct ConfirmedIdParams {
    /// The workspace's id, as workspaces_list names it.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct CreateParams {
    /// The name, unique without regard to case, at most 64 characters.
    pub name: String,
    /// What both panels show, as workspaces_get answers it in spec.
    pub spec: Value,
    /// The light mode's theme id (theme_list) while the workspace is active; the window's when
    /// omitted.
    #[serde(default)]
    pub theme_light: Option<String>,
    /// The dark mode's theme id while the workspace is active.
    #[serde(default)]
    pub theme_dark: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct CaptureParams {
    /// A stored workspace whose panels become what the panels show now; omitted, a new one.
    #[serde(default)]
    pub id: Option<i64>,
    /// The new workspace's name (required without id), or a new name for the workspace id.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct UpdateParams {
    pub id: i64,
    /// A new name; omitted, it stays.
    #[serde(default)]
    pub name: Option<String>,
    /// New panels, as workspaces_get answers them in spec; omitted, they stay.
    #[serde(default)]
    pub spec: Option<Value>,
    /// The light mode's theme id; an empty text leaves the light mode to the window; omitted,
    /// it stays.
    #[serde(default)]
    pub theme_light: Option<String>,
    /// The dark mode's theme id, as theme_light.
    #[serde(default)]
    pub theme_dark: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct RenameParams {
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct MoveParams {
    pub id: i64,
    /// Its place in the list, from 0; beyond the end is the last place.
    pub position: usize,
}

/// A theme change: an empty id leaves the mode to the window.
fn theme_change(id: Option<String>) -> Option<Option<String>> {
    id.map(|id| Some(id).filter(|id| !id.trim().is_empty()))
}

/// A refusal of the spec as a tool answer.
fn refused(error: &WorkspaceError) -> ToolResult {
    Ok(error_result(error.code(), &error.to_string()))
}

#[tool_router(router = workspaces_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read rCommander's workspaces in the person's order: each with id, name, spec (what both panels show: version, active panel left or right, split_permille, and per panel left and right its tabs with location, sort, mode and locked, active_tab, view files or terminal and terminal with kind local, wsl or ssh), theme_light and theme_dark (theme ids while it is active, null for the window's) and active (the one applied last)."
    )]
    pub(crate) async fn workspaces_list(&self) -> ToolResult {
        self.run(Command::ListWorkspaces {}).await
    }

    #[tool(
        description = "Read one workspace by its id from workspaces_list, as workspaces_list shows it. Error: workspace_unknown."
    )]
    pub(crate) async fn workspaces_get(
        &self,
        Parameters(params): Parameters<IdParams>,
    ) -> ToolResult {
        self.run(Command::GetWorkspace { id: params.id }).await
    }

    #[tool(
        description = "Save a new workspace after the last one: a name and spec, the panels as workspaces_get answers them (version 1; 1 to 32 tabs a panel on this machine or on a server of the connection tree, never inside an archive; at most 64 KiB), and optionally a theme per mode. A tab or a terminal's start folder on another machine's network share is refused with network_path, as a path of any tool is. Answers its id. Errors: workspace_name, workspace_name_taken, workspace_version, workspace_invalid, workspace_tabs, workspace_terminal, workspace_location, workspace_connection, workspace_theme, workspace_too_large."
    )]
    pub(crate) async fn workspaces_create(
        &self,
        Parameters(params): Parameters<CreateParams>,
    ) -> ToolResult {
        let spec = match WorkspaceSpec::from_value(params.spec) {
            Ok(spec) => spec,
            Err(error) => return refused(&error),
        };
        self.run(Command::CreateWorkspace(CreateWorkspaceRequest {
            name: params.name,
            spec,
            theme_light: params.theme_light,
            theme_dark: params.theme_dark,
        }))
        .await
    }

    #[tool(
        description = "Save what both panels show now - tabs, view, terminal, active panel, divider - as a new workspace named name, or over the panels of the workspace id (renamed when name is given). Answers its id. Errors: workspace_name, workspace_name_taken, workspace_unknown, workspace_tabs."
    )]
    pub(crate) async fn workspaces_capture(
        &self,
        Parameters(params): Parameters<CaptureParams>,
    ) -> ToolResult {
        self.run(Command::CaptureWorkspace {
            id: params.id,
            name: params.name,
        })
        .await
    }

    #[tool(
        description = "Change a workspace: what is given changes, the rest stays; an empty theme_light or theme_dark leaves that mode to the window. Errors as workspaces_create, and workspace_unknown."
    )]
    pub(crate) async fn workspaces_update(
        &self,
        Parameters(params): Parameters<UpdateParams>,
    ) -> ToolResult {
        let spec = match params.spec.map(WorkspaceSpec::from_value).transpose() {
            Ok(spec) => spec,
            Err(error) => return refused(&error),
        };
        self.run(Command::UpdateWorkspace(UpdateWorkspaceRequest {
            id: params.id,
            name: params.name,
            spec,
            theme_light: theme_change(params.theme_light),
            theme_dark: theme_change(params.theme_dark),
        }))
        .await
    }

    #[tool(
        description = "Give a workspace another name. Errors: workspace_unknown, workspace_name, workspace_name_taken."
    )]
    pub(crate) async fn workspaces_rename(
        &self,
        Parameters(params): Parameters<RenameParams>,
    ) -> ToolResult {
        self.run(Command::RenameWorkspace {
            id: params.id,
            name: params.name,
        })
        .await
    }

    #[tool(
        description = "Give a workspace another place in the list, from 0. Error: workspace_unknown."
    )]
    pub(crate) async fn workspaces_move(
        &self,
        Parameters(params): Parameters<MoveParams>,
    ) -> ToolResult {
        self.run(Command::MoveWorkspace {
            id: params.id,
            position: params.position,
        })
        .await
    }

    #[tool(description = "Delete a workspace. Error: workspace_unknown.")]
    pub(crate) async fn workspaces_delete(
        &self,
        Parameters(params): Parameters<ConfirmedIdParams>,
    ) -> ToolResult {
        self.run(Command::DeleteWorkspace { id: params.id }).await
    }

    #[tool(
        description = "Make both panels show a workspace: its tabs (after the setting workspaces.apply_mode: replace_all, the default, leaves each panel exactly the workspace's tabs; replace_keep_locked keeps the panels' locked tabs; add puts them after the panels'; a folder a panel shows already is never opened twice), the file list or its terminal (a running one of the same kind reused, else a shell started), the active panel and the divider; it becomes the active workspace. The person is asked with the workspace's name, each panel's tabs and the terminal it would show, and the call runs only while the workspace reads as shown (workspace_changed otherwise; a confirmation code for other tabs asks anew); a panel whose tabs do not fit a question (1000 characters) is refused with invalid_params and is the person's to apply. Answers skipped: the parts left out as <error code>:<what> - a connection deleted since, a server out of reach, a shell that did not start. Errors: workspace_unknown, workspace_changed."
    )]
    pub(crate) async fn workspaces_apply(
        &self,
        Parameters(params): Parameters<ConfirmedIdParams>,
    ) -> ToolResult {
        self.run(Command::ApplyWorkspace { id: params.id }).await
    }
}

#[cfg(test)]
#[path = "workspaces_tests.rs"]
mod tests;
