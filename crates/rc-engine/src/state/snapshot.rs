//! A panel as a command reports it ([`PanelSnapshot`]): everything but the entries themselves.

use rc_vfs::VfsPath;
use serde::{Deserialize, Serialize};

use super::{CursorView, ListMode, PanelId, PanelState, PanelView, Selection};
use crate::{FlatView, PanelTerminal, SortOrder};

/// A panel as a command reports it: everything but the entries themselves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelSnapshot {
    pub panel: PanelId,
    /// Whether keyboard input goes to this panel.
    pub active: bool,
    pub path: VfsPath,
    /// The entry under the cursor; `None` in an empty directory.
    pub cursor: Option<CursorView>,
    /// The selected names, sorted; the panel's own set, not a copy of it.
    pub selection: Selection,
    pub entry_count: usize,
    /// The file list or the terminal.
    pub view: PanelView,
    /// The order of the entries.
    pub sort: SortOrder,
    /// Full, brief, a column set or the comments (RC-100-90).
    pub mode: ListMode,
    /// Whether hidden and system entries are listed.
    pub show_hidden: bool,
    /// The quick filter in force; `entry_count` counts only what it shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// The flat view the panel shows, if any: what it lists and whether it is still reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flat: Option<FlatView>,
    /// For a panel showing its terminal, in `panel.get_state`: the shell's folder and prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<PanelTerminal>,
    /// The listing is still arriving: `entry_count` counts what came so far (RC-060-02).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub loading: bool,
    /// The branch of the folder's git repository, ahead and behind, once git answered with
    /// `panels.git_branch` on (RC-100-97).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<crate::git::GitBranch>,
}

impl PanelState {
    pub(crate) fn snapshot(&self, panel: PanelId, active: bool) -> PanelSnapshot {
        PanelSnapshot {
            panel,
            active,
            path: self.path.clone(),
            cursor: self.cursor_entry().map(|entry| CursorView {
                index: self.cursor,
                name: entry.name.clone(),
            }),
            selection: self.selection.clone(),
            entry_count: self.entries.len(),
            view: self.view,
            sort: self.sort,
            mode: self.mode,
            show_hidden: self.show_hidden,
            filter: self.filter.pattern.clone(),
            flat: self.flat.clone(),
            terminal: None,
            loading: self.listing.is_some(),
            git: self.git.as_ref().and_then(|git| git.branch.clone()),
        }
    }
}
