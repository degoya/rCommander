//! The parameters of the tab, history and hotlist commands, kept here so the [`crate::Command`]
//! variants stay one line each. Paths are absolute, or relative to the panel's folder; `panel:
//! None` is the active panel.

use serde::{Deserialize, Serialize};

use super::TabId;
use crate::PanelId;

/// Only a panel: `tabs.list`, `history.back`, `history.forward`, `history.list`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelRequest {
    pub panel: Option<PanelId>,
}

/// `tabs.open`: Ctrl+T (the panel's folder), Ctrl+Up (`cursor`), Ctrl+Shift+Up (`cursor` and
/// `other`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenTabRequest {
    /// The panel the folder comes from.
    pub panel: Option<PanelId>,
    /// The folder; when `None`, the one under the cursor with `cursor`, else the panel's own.
    pub path: Option<String>,
    /// The folder under the cursor, or the panel's own when the cursor is on a file.
    pub cursor: bool,
    /// The new tab opens in the other panel.
    pub other: bool,
}

/// `tabs.close` and `tabs.copy_to_other`: a tab, the active one when `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabRequest {
    pub panel: Option<PanelId>,
    pub tab: Option<TabId>,
}

/// `tabs.select`: the panel shows the tab `tab`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectTabRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    pub tab: TabId,
}

/// `tabs.next`: Ctrl+Tab, or Ctrl+Shift+Tab with `backwards`; round at the ends.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NextTabRequest {
    pub panel: Option<PanelId>,
    pub backwards: bool,
}

/// `tabs.lock`: lock (`Some(true)`), unlock, or switch (`None`) a tab, the active one when `tab`
/// is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LockTabRequest {
    pub panel: Option<PanelId>,
    pub tab: Option<TabId>,
    pub locked: Option<bool>,
}

/// `tabs.move`: the tab `tab` moves to place `to` of its row, counted from 0 among the tabs as
/// they stand after the move (beyond the last place it goes last); dragging a tab in its tab bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveTabRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    pub tab: TabId,
    pub to: usize,
}

/// `panel.swap`: Ctrl+U swaps the folders, Ctrl+Shift+U (`with_tabs`) the tab rows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwapRequest {
    pub with_tabs: bool,
}

/// `panel.transfer`: Ctrl+Left and Ctrl+Right show a folder of `panel` in panel `side`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDirRequest {
    /// The panel the folder comes from.
    #[serde(default)]
    pub panel: Option<PanelId>,
    /// The panel that shows it.
    pub side: PanelId,
    /// The folder; the one under the cursor when `None`, or the panel's own when the cursor is
    /// on a file.
    #[serde(default)]
    pub path: Option<String>,
}

/// `history.jump`: the active tab shows entry `index` of `history.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryJumpRequest {
    #[serde(default)]
    pub panel: Option<PanelId>,
    pub index: usize,
}

/// `hotlist.add`: an entry for a folder, by default the panel's own (Ctrl+D, "Add current
/// folder").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddHotlistRequest {
    /// The panel relative paths start from.
    pub panel: Option<PanelId>,
    /// The name; the folder's last part when `None`.
    pub name: Option<String>,
    pub path: Option<String>,
    /// A folder for the other panel, relative to the other panel's folder.
    pub target: Option<String>,
    /// The hotlist folder to add it to; the top when `None`.
    pub parent_id: Option<i64>,
}

/// `hotlist.add_folder`: a folder (a submenu) for entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddHotlistFolderRequest {
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<i64>,
}

/// `hotlist.update`: what is given changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateHotlistRequest {
    pub id: i64,
    /// The panel relative paths start from.
    #[serde(default)]
    pub panel: Option<PanelId>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    /// Removes the target; `target` is ignored then.
    #[serde(default)]
    pub clear_target: bool,
}

/// `hotlist.move`: into a hotlist folder (the top when `None`), at a place among its rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveHotlistRequest {
    pub id: i64,
    #[serde(default)]
    pub parent_id: Option<i64>,
    /// From 0; after the last row when `None`.
    #[serde(default)]
    pub position: Option<usize>,
}

/// `hotlist.open`: the panel shows the entry's folder, the other panel its target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenHotlistRequest {
    pub id: i64,
    #[serde(default)]
    pub panel: Option<PanelId>,
}
