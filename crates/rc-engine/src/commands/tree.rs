//! The folder trees (RC-100-88): `panel.show_tree` (Ctrl+F8) turns a panel into the tree of its
//! drive and back, `panel.cycle_separate_tree` (Ctrl+Shift+F8) turns the tree beside the panels
//! off, one, two and off. The levels every tree shows come from [`Engine::tree_children`], no
//! command: MCP reads folders with `list_dir`.

use rc_vfs::VfsPath;
use serde_json::Value;

use crate::{
    Engine, EngineError, Outcome, PanelId, PanelView,
    settings::{SEPARATE_TREE_KEY, stored_setting},
    tree::{SeparateTree, TreeLevel, read_level},
};

impl Engine {
    /// `panel.show_tree`: the panel shows the tree of its drive (`show` true), its files again
    /// (false), or the other of the two (`None`). A panel showing its terminal turns to the
    /// tree; its shell keeps running.
    pub(crate) fn show_tree(&self, panel: Option<PanelId>, show: Option<bool>) -> Outcome {
        let id = self.resolve(panel);
        let tree = self.state().panel(id).view == PanelView::Tree;
        let show = show.unwrap_or(!tree);
        if show == tree {
            return Outcome::Panel(self.snapshot(Some(id)));
        }
        let view = if show {
            PanelView::Tree
        } else {
            PanelView::Files
        };
        self.set_view(id, view, None)
    }

    /// A panel that shows its tree shows its files: Ctrl+F1, Ctrl+F2 and a drive change end the
    /// tree, as in Total Commander. `outcome` is the command's, the panel's state after it.
    pub(crate) fn leave_tree(&self, outcome: Outcome) -> Outcome {
        match outcome {
            Outcome::Panel(snapshot) if snapshot.view == PanelView::Tree => {
                self.set_view(snapshot.panel, PanelView::Files, None)
            }
            other => other,
        }
    }

    /// `panel.cycle_separate_tree`: `panels.separate_tree` from off to one, two and off again.
    ///
    /// # Errors
    ///
    /// No database is attached, or it fails.
    pub(crate) async fn cycle_separate_tree(&self) -> Result<Outcome, EngineError> {
        let now = stored_setting(self.db()?, SEPARATE_TREE_KEY)
            .await
            .as_ref()
            .and_then(Value::as_str)
            .map(SeparateTree::from_setting)
            .unwrap_or_default();
        let next = Value::from(now.next().as_str());
        self.set_setting(SEPARATE_TREE_KEY, &next).await
    }

    /// The sub-folders of `path` for a folder tree, hidden ones only while the panels show hidden
    /// entries (RC-100-88): the level kept from before, else one listing of `path` - on the
    /// backend's session for a server, never a walk below it. At most [`crate::tree::MAX_LEVEL`]
    /// sub-folders, `truncated` past that.
    ///
    /// # Errors
    ///
    /// The backend is not open (a server whose session went), or the folder does not list.
    pub async fn tree_children(&self, path: &VfsPath) -> Result<TreeLevel, EngineError> {
        let show_hidden = self.state().show_hidden;
        if let Some(level) = self.inner.tree.get(path) {
            return Ok(level.shown(show_hidden));
        }
        let vfs = self.open_vfs(path, false).await?;
        let level = read_level(&*vfs, path).await?;
        self.inner.tree.put(&level);
        Ok(level.shown(show_hidden))
    }

    /// A panel follows a tree's cursor (RC-100-88): `panel` shows `path` on whatever backend it
    /// lies - a tree on a server or in an archive leads its panel there. The window's, as
    /// [`Engine::tree_children`] is; MCP moves a panel with `navigate`.
    ///
    /// # Errors
    ///
    /// The folder does not list.
    pub async fn show_folder(&self, panel: PanelId, path: VfsPath) -> Result<Outcome, EngineError> {
        self.show_path(panel, path).await
    }

    /// F2 in a tree: the level of `path` is read again at its next use.
    pub fn tree_reread(&self, path: &VfsPath) {
        self.inner.tree.stale(path);
    }

    /// A tree folded `path`: what is kept below it may go first when the limit is reached.
    pub fn tree_folded(&self, path: &VfsPath) {
        self.inner.tree.folded(path);
    }

    /// A tree unfolded `path` again.
    pub fn tree_unfolded(&self, path: &VfsPath) {
        self.inner.tree.unfolded(path);
    }

    /// The folders of `paths` and the folders holding them are read again at their next use in
    /// a tree: what a file operation changed (`refresh_showing`, `panel.refresh`).
    pub(crate) fn trees_changed(&self, paths: &[VfsPath]) {
        for path in paths {
            self.inner.tree.stale(path);
            if let Some(parent) = path.parent() {
                self.inner.tree.stale(&parent);
            }
        }
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
