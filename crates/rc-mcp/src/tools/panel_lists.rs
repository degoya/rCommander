//! The `panel_lists` tool group (RC-100-92): Total Commander's smaller panel functions for agents:
//! the file filters (Ctrl+F10, Ctrl+F11, Ctrl+F12) and the saved ones, the ignore list's switch,
//! the selection saved and loaded (memory, a file, a text), a list file shown in a panel, a tab's
//! own name, closing all tabs and the most visited folders. None is a trust decision.

use rc_engine::{Command, FileFilter, TabId};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{
    RcMcpServer,
    answer::ToolResult,
    tools::panels::{PanelArg, PanelParams},
};

/// Which files a panel shows.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FilterKind {
    /// Every file (Ctrl+F10).
    All,
    /// Programs only (Ctrl+F11): the extensions of the setting filters.programs, off Windows also
    /// files with an execute bit.
    Programs,
    /// The files `mask` matches (Ctrl+F12).
    Mask,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FileFilterParams {
    pub kind: FilterKind,
    /// With kind mask: a mask line as Num+ takes it, `*.txt;*.md|read*` (masks after `|` are left
    /// out), case ignored; at most 1024 characters.
    #[serde(default)]
    pub mask: Option<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SaveFilterParams {
    /// At most 64 characters; a filter of the same name (ignoring case) is replaced.
    pub name: String,
    /// A mask line, at most 1024 characters.
    pub mask: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct RenameFilterParams {
    pub name: String,
    pub new_name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FilterNameParams {
    pub name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct IgnoreListParams {
    /// true switches the ignore list on, false off; omitted switches.
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct ListFileParams {
    /// A text file on this machine: absolute, or relative to the panel's folder.
    pub path: String,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct SelectionTextParams {
    /// Names, one per line (a name in double quotes may hold spaces; in a flat view the path
    /// below its folder, or an absolute path below the panel's folder); at most 16 MiB.
    pub text: String,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TabRenameParams {
    /// A tab id from tab_list; the panel's shown tab when omitted.
    #[serde(default)]
    pub tab: Option<u64>,
    /// The tab's own name, at most 64 characters; omitted or blank shows the folder's name again.
    #[serde(default)]
    pub title: Option<String>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TabCloseAllParams {
    /// Close the locked tabs too.
    #[serde(default)]
    pub include_locked: bool,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct OpenFrequentParams {
    /// An index into history_list's frequent.
    pub index: usize,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

/// The filter `kind` asks for.
fn filter(kind: FilterKind, mask: Option<String>) -> FileFilter {
    match kind {
        FilterKind::All => FileFilter::All,
        FilterKind::Programs => FileFilter::Programs,
        FilterKind::Mask => FileFilter::Mask(mask.unwrap_or_default()),
    }
}

#[tool_router(router = panel_lists_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "Set a panel's file filter as Total Commander's Ctrl+F10 (kind all, which also lifts the quick filter), Ctrl+F11 (programs) or Ctrl+F12 (mask): only the files it takes show, folders always. Unlike panel_filter it stays when the panel goes to another folder, and it belongs to the shown tab (with tabs.filter_per_tab off, to every tab of the panel). Selected files it hides are deselected. Answers the panel's state. Errors: filter_invalid."
    )]
    pub(crate) async fn panel_file_filter(
        &self,
        Parameters(params): Parameters<FileFilterParams>,
    ) -> ToolResult {
        let FileFilterParams { kind, mask, panel } = params;
        self.run(Command::SetFileFilter {
            panel: panel.map(Into::into),
            filter: Some(filter(kind, mask)),
        })
        .await
    }

    #[tool(
        description = "The saved file filters (Ctrl+F12's list): name and mask, sorted by name."
    )]
    pub(crate) async fn filters_list(&self) -> ToolResult {
        self.run(Command::ListFileFilters).await
    }

    #[tool(
        description = "Save a file filter's mask under a name, replacing one of the same name (ignoring case); at most 50 are kept. Answers the saved filters. Errors: saved_filter_invalid, filter_invalid."
    )]
    pub(crate) async fn filters_save(
        &self,
        Parameters(params): Parameters<SaveFilterParams>,
    ) -> ToolResult {
        self.run(Command::SaveFileFilter {
            name: params.name,
            mask: params.mask,
        })
        .await
    }

    #[tool(
        description = "Give a saved file filter another name. Answers the saved filters. Errors: saved_filter_not_found, saved_filter_invalid (bad name, or another filter has it)."
    )]
    pub(crate) async fn filters_rename(
        &self,
        Parameters(params): Parameters<RenameFilterParams>,
    ) -> ToolResult {
        self.run(Command::RenameFileFilter {
            name: params.name,
            new_name: params.new_name,
        })
        .await
    }

    #[tool(
        description = "Delete a saved file filter by its name. Answers the saved filters. Errors: saved_filter_not_found."
    )]
    pub(crate) async fn filters_delete(
        &self,
        Parameters(params): Parameters<FilterNameParams>,
    ) -> ToolResult {
        self.run(Command::DeleteFileFilter { name: params.name })
            .await
    }

    #[tool(
        description = "Switch the ignore list (the setting ignore_list.entries) on or off, as the setting ignore_list.enabled: the names, masks and paths it lists never show in the panels, the flat views and the search, and copies and moves leave them where they are. Both panels are read again. Answers the setting."
    )]
    pub(crate) async fn panel_toggle_ignore_list(
        &self,
        Parameters(params): Parameters<IgnoreListParams>,
    ) -> ToolResult {
        self.run(Command::ToggleIgnoreList {
            enabled: params.enabled,
        })
        .await
    }

    #[tool(
        description = "Keep a panel's selection in its memory (Total Commander's Save selection); panel_select with restore (Num /) brings it back. Answers the panel's state."
    )]
    pub(crate) async fn panel_save_selection(
        &self,
        Parameters(params): Parameters<PanelParams>,
    ) -> ToolResult {
        self.run(Command::SaveSelection {
            panel: params.panel.map(Into::into),
        })
        .await
    }

    #[tool(
        description = "Write the names of a panel's selection to a new text file on this machine, UTF-8, one per line in the panel's order (in a flat view the path below its folder). An existing file is never overwritten. Answers the file and the number of names. Errors: file_exists, list_file_not_local, permission_denied."
    )]
    pub(crate) async fn panel_selection_to_file(
        &self,
        Parameters(params): Parameters<ListFileParams>,
    ) -> ToolResult {
        self.run(Command::SelectionToFile {
            panel: params.panel.map(Into::into),
            path: params.path,
            overwrite: false,
        })
        .await
    }

    #[tool(
        description = "Select the entries a text file on this machine names, one per line (UTF-8, UTF-16 with a byte-order mark or ANSI; at most 16 MiB); the selection grows. Answers the panel, how many were selected and how many named entries the panel does not list (missing). Errors: not_found, list_file_too_large, list_file_not_local."
    )]
    pub(crate) async fn panel_selection_from_file(
        &self,
        Parameters(params): Parameters<ListFileParams>,
    ) -> ToolResult {
        self.run(Command::SelectionFromFile {
            panel: params.panel.map(Into::into),
            path: params.path,
        })
        .await
    }

    #[tool(
        description = "Select the entries a text names, one per line, as Total Commander's Load selection from clipboard; the selection grows. Answers the panel, selected and missing. Errors: list_file_too_large."
    )]
    pub(crate) async fn panel_selection_from_text(
        &self,
        Parameters(params): Parameters<SelectionTextParams>,
    ) -> ToolResult {
        self.run(Command::SelectionFromText {
            panel: params.panel.map(Into::into),
            text: params.text,
        })
        .await
    }

    #[tool(
        description = "Show the absolute paths of a list file on this machine (one per line, a path in double quotes allowed) in a panel as a flat view below their common folder, as Total Commander's LOADLIST; paths that are not there are skipped and counted in the flat view's skipped. Answers the panel's state. Errors: list_file_empty, list_file_too_large, list_file_not_local, not_found."
    )]
    pub(crate) async fn panel_load_list(
        &self,
        Parameters(params): Parameters<ListFileParams>,
    ) -> ToolResult {
        self.run(Command::LoadList {
            panel: params.panel.map(Into::into),
            path: params.path,
        })
        .await
    }

    #[tool(
        description = "Give a tab its own name (Total Commander's Rename tab), kept in every folder it shows and with the tabs; omitted or blank shows the folder's name again. Answers the panel's tabs. Errors: tab_title_invalid, tab_not_found."
    )]
    pub(crate) async fn tab_rename(
        &self,
        Parameters(params): Parameters<TabRenameParams>,
    ) -> ToolResult {
        self.run(Command::RenameTab {
            panel: params.panel.map(Into::into),
            tab: params.tab.map(TabId),
            title: params.title,
        })
        .await
    }

    #[tool(
        description = "Close every tab of a panel but the shown one; locked tabs stay unless include_locked. Answers the panel's tabs."
    )]
    pub(crate) async fn tab_close_all(
        &self,
        Parameters(params): Parameters<TabCloseAllParams>,
    ) -> ToolResult {
        self.run(Command::CloseAllTabs {
            panel: params.panel.map(Into::into),
            include_locked: params.include_locked,
        })
        .await
    }

    #[tool(
        description = "Show one of the most visited folders by its index in history_list's frequent (counts halve every 30 days; the setting history.frequent_shown says how many are kept, 0 none). A locked tab opens it in a new tab. Answers the panel's state. Errors: history_out_of_range, not_found."
    )]
    pub(crate) async fn history_open_frequent(
        &self,
        Parameters(params): Parameters<OpenFrequentParams>,
    ) -> ToolResult {
        self.run(Command::HistoryOpenFrequent {
            panel: params.panel.map(Into::into),
            index: params.index,
        })
        .await
    }
}

#[cfg(test)]
#[path = "panel_lists_tests.rs"]
mod tests;
