//! The `tabs` tool group: the panels' folder tabs (`tabs.*`), each tab's folder history
//! (`history.*`) and the moves between the panels (`panel.swap`, `panel.transfer`), RC-010-03.
//! Stepping to the next tab (`tabs.next`) is a key; `tab_select` names the tab.

use rc_engine::{
    Command, HistoryJumpRequest, LockTabRequest, MoveTabRequest, OpenTabRequest, PanelRequest,
    SelectTabRequest, SwapRequest, TabId, TabRequest, TransferDirRequest,
};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;

use crate::{
    RcMcpServer,
    answer::ToolResult,
    tools::panels::{PanelArg, PanelParams},
};

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TabOpenParams {
    /// The folder: absolute, or relative to the panel's folder. Omitted: the panel's own folder,
    /// or with `cursor` the folder under the panel's cursor.
    #[serde(default)]
    pub path: Option<String>,
    /// Open the folder under the panel's cursor (the panel's own when the cursor is on a file).
    #[serde(default)]
    pub cursor: bool,
    /// Open the tab in the other panel.
    #[serde(default)]
    pub other_panel: bool,
    /// `left` or `right`, the panel the folder comes from; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TabParams {
    /// A tab id from tab_list; the panel's shown tab when omitted.
    #[serde(default)]
    pub tab: Option<u64>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TabSelectParams {
    /// A tab id from tab_list.
    pub tab: u64,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TabLockParams {
    /// A tab id from tab_list; the panel's shown tab when omitted.
    #[serde(default)]
    pub tab: Option<u64>,
    /// true locks, false unlocks; omitted switches.
    #[serde(default)]
    pub locked: Option<bool>,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct TabMoveParams {
    /// A tab id from tab_list.
    pub tab: u64,
    /// The tab's new place in its row, from 0, counted as the row stands after the move;
    /// beyond the last place it goes last.
    pub to: usize,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct PanelSwapParams {
    /// Swap the whole tab rows (Ctrl+Shift+U) instead of the shown folders (Ctrl+U).
    #[serde(default)]
    pub with_tabs: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct PanelTransferParams {
    /// `left` or `right`: the panel that shows the folder.
    pub to: PanelArg,
    /// The folder: absolute, or relative to the `from` panel's folder. Omitted: the folder under
    /// the `from` panel's cursor, or its own folder when the cursor is on a file.
    #[serde(default)]
    pub path: Option<String>,
    /// `left` or `right`, the panel the folder comes from; the active panel when omitted.
    #[serde(default)]
    pub from: Option<PanelArg>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct HistoryJumpParams {
    /// An index into the entries of history_list.
    pub index: usize,
    /// `left` or `right`; the active panel when omitted.
    #[serde(default)]
    pub panel: Option<PanelArg>,
}

fn panel_request(params: PanelParams) -> PanelRequest {
    PanelRequest {
        panel: params.panel.map(Into::into),
    }
}

#[tool_router(router = tabs_router, vis = "pub(crate)")]
impl RcMcpServer {
    #[tool(
        description = "List a panel's folder tabs, left to right: id, title (the folder's last part, told apart by its parent or a number where two are equal), path, locked, active (the tab the panel shows)."
    )]
    pub(crate) async fn tab_list(&self, Parameters(params): Parameters<PanelParams>) -> ToolResult {
        self.run(Command::ListTabs(panel_request(params))).await
    }

    #[tool(
        description = "Open a folder in a new tab right of the shown one, which the panel then shows, as Ctrl+T (the panel's own folder), Ctrl+Up (cursor: the folder under the cursor) or Ctrl+Shift+Up (cursor and other_panel). Answers the tabs of the panel that got the tab. Errors: not_found, not_a_directory, permission_denied."
    )]
    pub(crate) async fn tab_open(
        &self,
        Parameters(params): Parameters<TabOpenParams>,
    ) -> ToolResult {
        self.run(Command::OpenTab(OpenTabRequest {
            panel: params.panel.map(Into::into),
            path: params.path,
            cursor: params.cursor,
            other: params.other_panel,
        }))
        .await
    }

    #[tool(
        description = "Close a tab, the shown one by default (Ctrl+W); closing the shown tab shows its right neighbour, or its left one at the end. Answers the panel's tabs. Errors: last_tab (a panel keeps one tab), tab_not_found."
    )]
    pub(crate) async fn tab_close(&self, Parameters(params): Parameters<TabParams>) -> ToolResult {
        self.run(Command::CloseTab(TabRequest {
            panel: params.panel.map(Into::into),
            tab: params.tab.map(TabId),
        }))
        .await
    }

    #[tool(
        description = "Show a tab of a panel by its id: the panel returns to the tab's folder, sort, view mode and cursor; a folder that is gone shows the nearest folder above it. Answers the panel's tabs. Errors: tab_not_found."
    )]
    pub(crate) async fn tab_select(
        &self,
        Parameters(params): Parameters<TabSelectParams>,
    ) -> ToolResult {
        self.run(Command::SelectTab(SelectTabRequest {
            panel: params.panel.map(Into::into),
            tab: TabId(params.tab),
        }))
        .await
    }

    #[tool(
        description = "Lock or unlock a tab, the shown one by default. A locked tab keeps its folder: navigating it opens the new folder in a new tab. Answers the panel's tabs. Errors: tab_not_found."
    )]
    pub(crate) async fn tab_lock(
        &self,
        Parameters(params): Parameters<TabLockParams>,
    ) -> ToolResult {
        self.run(Command::LockTab(LockTabRequest {
            panel: params.panel.map(Into::into),
            tab: params.tab.map(TabId),
            locked: params.locked,
        }))
        .await
    }

    #[tool(
        description = "Copy a tab, the shown one by default, to the end of the other panel's tabs, unlocked; the other panel keeps showing its own tab. Answers the other panel's tabs. Errors: tab_not_found."
    )]
    pub(crate) async fn tab_copy_to_other(
        &self,
        Parameters(params): Parameters<TabParams>,
    ) -> ToolResult {
        self.run(Command::CopyTabToOther(TabRequest {
            panel: params.panel.map(Into::into),
            tab: params.tab.map(TabId),
        }))
        .await
    }

    #[tool(
        description = "Move a tab to another place in its panel's row, as dragging it in the tab bar does: to is its new place from 0 (beyond the last place it goes last). The panel keeps showing the tab it showed; the order is stored with the tabs. Answers the panel's tabs. Errors: tab_not_found."
    )]
    pub(crate) async fn tab_move(
        &self,
        Parameters(params): Parameters<TabMoveParams>,
    ) -> ToolResult {
        self.run(Command::MoveTab(MoveTabRequest {
            panel: params.panel.map(Into::into),
            tab: TabId(params.tab),
            to: params.to,
        }))
        .await
    }

    #[tool(
        description = "Swap the two panels' folders with cursor, selection, sort and view mode (Ctrl+U), or with with_tabs their whole tab rows (Ctrl+Shift+U). Each panel keeps showing its files or its terminal. Answers the active panel's new state."
    )]
    pub(crate) async fn panel_swap(
        &self,
        Parameters(params): Parameters<PanelSwapParams>,
    ) -> ToolResult {
        self.run(Command::SwapPanels(SwapRequest {
            with_tabs: params.with_tabs,
        }))
        .await
    }

    #[tool(
        description = "Show a folder of one panel in a panel, as Ctrl+Left and Ctrl+Right: the folder under the from panel's cursor, its own folder when the cursor is on a file, or path. A locked tab in the to panel opens it in a new tab. Errors: not_found, not_a_directory, permission_denied."
    )]
    pub(crate) async fn panel_transfer(
        &self,
        Parameters(params): Parameters<PanelTransferParams>,
    ) -> ToolResult {
        self.run(Command::TransferDir(TransferDirRequest {
            panel: params.from.map(Into::into),
            side: params.to.into(),
            path: params.path,
        }))
        .await
    }

    #[tool(
        description = "Go back in the shown tab's folder history (Alt+Left). Answers the panel's new state. Errors: history_end (nothing further back), not_found (the folder is gone; the history keeps its place)."
    )]
    pub(crate) async fn history_back(
        &self,
        Parameters(params): Parameters<PanelParams>,
    ) -> ToolResult {
        self.run(Command::HistoryBack(panel_request(params))).await
    }

    #[tool(
        description = "Go forward in the shown tab's folder history (Alt+Right); a new folder after going back drops what lay ahead. Answers the panel's new state. Errors: history_end, not_found."
    )]
    pub(crate) async fn history_forward(
        &self,
        Parameters(params): Parameters<PanelParams>,
    ) -> ToolResult {
        self.run(Command::HistoryForward(panel_request(params)))
            .await
    }

    #[tool(
        description = "List the shown tab's folder history (Alt+Down): entries oldest first (at most 50) and current, the index of the folder shown."
    )]
    pub(crate) async fn history_list(
        &self,
        Parameters(params): Parameters<PanelParams>,
    ) -> ToolResult {
        self.run(Command::HistoryList(panel_request(params))).await
    }

    #[tool(
        description = "Show a folder of the shown tab's history by its index in history_list; the history keeps its entries. Errors: history_out_of_range, not_found."
    )]
    pub(crate) async fn history_jump(
        &self,
        Parameters(params): Parameters<HistoryJumpParams>,
    ) -> ToolResult {
        self.run(Command::HistoryJump(HistoryJumpRequest {
            panel: params.panel.map(Into::into),
            index: params.index,
        }))
        .await
    }
}

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;
