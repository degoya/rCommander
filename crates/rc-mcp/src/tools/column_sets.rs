//! The `column_sets` tool group (RC-100-90): thin wrappers over the `column_sets.*` commands. A
//! column set is the file list's own columns - each a field (name, ext, size, packed, modified,
//! created, accessed, attributes, rights, owner, group, comment, git) with a title, a width and an
//! alignment - saved under a name, with folder masks that switch a panel entering a matching
//! folder to it. `panel_set_view` with `custom` shows one.

use rc_engine::{ColumnSetError, ColumnSetSpec, Command};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    RcMcpServer,
    answer::{ToolResult, error_result},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct IdParams {
    /// The column set's id, as column_sets_list names it.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct ConfirmedIdParams {
    /// The column set's id, as column_sets_list names it.
    pub id: i64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct CreateParams {
    /// The name, unique without regard to case, at most 64 characters.
    pub name: String,
    /// The columns and masks, as column_sets_get answers them in spec.
    pub spec: Value,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[schemars(transform = crate::confirm::argument)]
pub(crate) struct UpdateParams {
    pub id: i64,
    /// The new columns and masks, as column_sets_get answers them in spec.
    pub spec: Value,
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

/// A refusal of the spec as a tool answer.
fn refused(error: &ColumnSetError) -> ToolResult {
    Ok(error_result(error.code(), &error.to_string()))
}

#[tool_router(router = column_sets_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Read rCommander's column sets (Total Commander's custom columns) in the person's order: each with id, name and spec (version 1; columns, each with field - name, ext, size, packed, modified, created, accessed, attributes, rights, owner, group, comment or git (the git status as a word) -, title (null: the field's name), width in pixels 24 to 640 and align left or right; masks, the folder masks that switch a panel to the set, the first set in order winning). panel_set_view with custom and set shows one."
    )]
    pub(crate) async fn column_sets_list(&self) -> ToolResult {
        self.run(Command::ListColumnSets {}).await
    }

    #[tool(
        description = "Read one column set by its id from column_sets_list, as column_sets_list shows it. Error: column_set_unknown."
    )]
    pub(crate) async fn column_sets_get(
        &self,
        Parameters(params): Parameters<IdParams>,
    ) -> ToolResult {
        self.run(Command::GetColumnSet { id: params.id }).await
    }

    #[tool(
        description = "Save a new column set after the last one: a name and spec as column_sets_get answers it (version 1; 1 to 16 columns; 0 to 32 folder masks in Total Commander's syntax such as D:\\Photos\\* or sftp://*, matched against the path as the path line shows it; at most 16 KiB; at most 64 sets). Answers its id. Errors: column_set_name, column_set_name_taken, column_set_version, column_set_invalid, column_set_too_large, column_set_too_many."
    )]
    pub(crate) async fn column_sets_create(
        &self,
        Parameters(params): Parameters<CreateParams>,
    ) -> ToolResult {
        let spec = match ColumnSetSpec::from_value(params.spec) {
            Ok(spec) => spec,
            Err(error) => return refused(&error),
        };
        self.run(Command::CreateColumnSet {
            name: params.name,
            spec,
        })
        .await
    }

    #[tool(
        description = "Replace a column set's columns and folder masks; a panel showing it draws the new columns. Errors as column_sets_create, and column_set_unknown."
    )]
    pub(crate) async fn column_sets_update(
        &self,
        Parameters(params): Parameters<UpdateParams>,
    ) -> ToolResult {
        let spec = match ColumnSetSpec::from_value(params.spec) {
            Ok(spec) => spec,
            Err(error) => return refused(&error),
        };
        self.run(Command::UpdateColumnSet {
            id: params.id,
            spec,
        })
        .await
    }

    #[tool(
        description = "Give a column set another name. Errors: column_set_name, column_set_name_taken, column_set_unknown."
    )]
    pub(crate) async fn column_sets_rename(
        &self,
        Parameters(params): Parameters<RenameParams>,
    ) -> ToolResult {
        self.run(Command::RenameColumnSet {
            id: params.id,
            name: params.name,
        })
        .await
    }

    #[tool(
        description = "Give a column set another place in the list, from 0; the order decides whose folder mask wins. Error: column_set_unknown."
    )]
    pub(crate) async fn column_sets_move(
        &self,
        Parameters(params): Parameters<MoveParams>,
    ) -> ToolResult {
        self.run(Command::MoveColumnSet {
            id: params.id,
            position: params.position,
        })
        .await
    }

    #[tool(
        description = "Delete a column set; a panel or tab showing it shows the full view. Error: column_set_unknown."
    )]
    pub(crate) async fn column_sets_delete(
        &self,
        Parameters(params): Parameters<ConfirmedIdParams>,
    ) -> ToolResult {
        self.run(Command::DeleteColumnSet { id: params.id }).await
    }
}

#[cfg(test)]
#[path = "column_sets_tests.rs"]
mod tests;
