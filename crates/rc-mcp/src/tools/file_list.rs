//! The file-list tools: a panel's order, its full or brief view, hidden entries, the selection by
//! mask (Num+, Num-) or extension, and opening an entry as Enter does.

use rc_engine::{Command, ListMode, SortKey};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use super::panels::PanelArg;
use crate::{RcMcpServer, answer::ToolResult};

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SortKeyArg {
    Name,
    Ext,
    Size,
    Date,
}

impl From<SortKeyArg> for SortKey {
    fn from(key: SortKeyArg) -> Self {
        match key {
            SortKeyArg::Name => Self::Name,
            SortKeyArg::Ext => Self::Ext,
            SortKeyArg::Size => Self::Size,
            SortKeyArg::Date => Self::Date,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ModeArg {
    Full,
    Brief,
}

/// What `panel_select` does.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SelectAction {
    /// Select every entry, directories included.
    All,
    /// Deselect every entry.
    None,
    /// Select what is not selected and deselect the rest.
    Invert,
    /// Select every file with the extension of the file under the cursor.
    SelectExtension,
    /// Deselect every file with the extension of the file under the cursor.
    DeselectExtension,
    /// Select again what was selected before the last file operation.
    Restore,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SortParams {
    /// `name`, `ext`, `size` or `date`.
    pub key: SortKeyArg,
    /// Omitted: the panel's key again turns the direction around, another key starts largest
    /// and newest first for size and date, from A for name and ext.
    #[serde(default)]
    pub descending: Option<bool>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SetViewParams {
    /// `full` (name, ext, size, date columns) or `brief` (names only, several columns).
    pub mode: ModeArg,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ShowHiddenParams {
    /// `true` shows hidden and system entries, `false` hides them; omitted, it switches.
    #[serde(default)]
    pub show: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SelectParams {
    pub action: SelectAction,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct MaskParams {
    /// Total Commander masks: several separated by `;`, `*` and `?` as wildcards, masks after
    /// `|` excluded (`*.txt;*.md|draft*`). They match files only; a mask ending in `\` matches
    /// directories. Omitted: `*.*`, every file.
    #[serde(default)]
    pub mask: Option<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct OpenParams {
    /// Absolute, or relative to the panel's directory; the entry under the cursor when omitted.
    #[serde(default)]
    pub path: Option<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// The command for `panel_select`.
fn select_command(params: SelectParams) -> Command {
    let panel = params.panel.map(Into::into);
    match params.action {
        SelectAction::All => Command::SelectAll { panel },
        SelectAction::None => Command::DeselectAll { panel },
        SelectAction::Invert => Command::InvertSelection { panel },
        SelectAction::SelectExtension | SelectAction::DeselectExtension => {
            Command::SelectExtension {
                panel,
                select: matches!(params.action, SelectAction::SelectExtension),
            }
        }
        SelectAction::Restore => Command::RestoreSelection { panel },
    }
}

#[tool_router(router = file_list_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Sort a panel's entries by name, ext, size or date; directories stay on top and names compare naturally (file2 before file10), ignoring case. Without descending the same key again turns the direction around. The cursor stays on its entry. Answers the panel's state with its sort."
    )]
    pub(crate) async fn panel_sort(
        &self,
        Parameters(params): Parameters<SortParams>,
    ) -> ToolResult {
        self.run(Command::Sort {
            panel: params.panel.map(Into::into),
            key: params.key.into(),
            descending: params.descending,
        })
        .await
    }

    #[tool(
        description = "Show a panel in the full view (name, ext, size, date columns) or the brief view (names only, in several columns). Answers the panel's state."
    )]
    pub(crate) async fn panel_set_view(
        &self,
        Parameters(params): Parameters<SetViewParams>,
    ) -> ToolResult {
        self.run(Command::SetListMode {
            panel: params.panel.map(Into::into),
            mode: match params.mode {
                ModeArg::Full => ListMode::Full,
                ModeArg::Brief => ListMode::Brief,
            },
        })
        .await
    }

    #[tool(
        description = "Show or hide hidden and system entries (dot files; the hidden or system attribute on Windows) in both panels; the choice is kept for the next start. list_dir always lists them, marked hidden. Answers the active panel's state."
    )]
    pub(crate) async fn panel_show_hidden(
        &self,
        Parameters(params): Parameters<ShowHiddenParams>,
    ) -> ToolResult {
        self.run(Command::ToggleHidden { show: params.show }).await
    }

    #[tool(
        description = "Change a panel's whole selection: all, none, invert, select_extension or deselect_extension (files with the extension of the file under the cursor), restore (the selection before the last file operation). Answers the panel's state with the selected names."
    )]
    pub(crate) async fn panel_select(
        &self,
        Parameters(params): Parameters<SelectParams>,
    ) -> ToolResult {
        self.run(select_command(params)).await
    }

    #[tool(
        description = "Select the files of a panel that a Total Commander mask matches, as Num+ does: *.txt;*.doc, several masks separated by ;, masks after | excluded, a trailing backslash for directories (files only otherwise; with the setting selection.include_folders on, a mask without a trailing backslash matches directories by the same names too, never ..). Omitted mask: every file. Adds to the selection; answers the panel's state."
    )]
    pub(crate) async fn panel_select_by_mask(
        &self,
        Parameters(params): Parameters<MaskParams>,
    ) -> ToolResult {
        self.run(Command::SelectByMask(rc_engine::MaskRequest {
            panel: params.panel.map(Into::into),
            mask: params.mask,
        }))
        .await
    }

    #[tool(
        description = "Deselect the files of a panel that a Total Commander mask matches, as Num- does; the same masks as panel_select_by_mask, directories included as there under selection.include_folders. Answers the panel's state."
    )]
    pub(crate) async fn panel_deselect_by_mask(
        &self,
        Parameters(params): Parameters<MaskParams>,
    ) -> ToolResult {
        self.run(Command::DeselectByMask(rc_engine::MaskRequest {
            panel: params.panel.map(Into::into),
            mask: params.mask,
        }))
        .await
    }

    #[tool(
        description = "Open an entry as Enter does in the window: a directory is shown in the panel; a program starts and any other file opens in the program the system associates with it. path is absolute or relative to the panel's directory; omitted, the entry under the cursor. Local files only. Without a path the question names the entry under the cursor, and that entry is opened even if the cursor moves before the person allows it. Errors: not_found, unsupported, open_failed, panel_changed (the panel shows another folder than the question did)."
    )]
    pub(crate) async fn file_open(&self, Parameters(params): Parameters<OpenParams>) -> ToolResult {
        self.run(Command::OpenFile {
            panel: params.panel.map(Into::into),
            path: params.path,
        })
        .await
    }
}

#[cfg(test)]
#[path = "file_list_tests.rs"]
mod tests;
