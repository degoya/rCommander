//! `view.open` (F3) and `panel.toggle_quick_view` (Ctrl+Q): the window's viewer is wanted. The
//! engine names the file and announces it; reading and drawing are the window's, through
//! [`Engine::open_for_view`] and the GUI-free model in [`crate::viewer`].

use rc_vfs::{VfsError, VfsPath};
use serde::{Deserialize, Serialize};

use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelId, settings::STEP_THROUGH_SELECTION_KEY,
    viewer::FileWindow,
};

/// Which files N and P step through in a Lister (RC-100-91): the window keeps the list, the
/// engine says which it is when it asks for the Lister.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListerFiles {
    /// Every file of the panel: nothing is marked, or `viewer.step_through_selection` is off.
    #[default]
    Panel,
    /// The panel's marked files only: F3 with files marked, as Total Commander's Lister.
    Marked,
    /// This file alone, N and P stay on it: Shift+F3 with files marked.
    Single,
}

impl Engine {
    /// `view.open`: `path` (resolved against the panel's directory, or a server's location as
    /// `ops.copy` takes it), or the entry under the cursor, announced for the window's viewer. A
    /// directory is refused (`is_a_directory`); an empty panel answers itself unchanged.
    pub(crate) async fn view_file(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (directory, cursor) = {
            let state = self.state();
            let panel = state.panel(id);
            (panel.path.clone(), panel.cursor_entry().cloned())
        };
        let target = match (path, cursor) {
            (Some(path), _) => self.locate(&directory, path).await?,
            (None, Some(entry)) => directory.join(&entry.name),
            (None, None) => return Ok(Outcome::Panel(self.snapshot(Some(id)))),
        };
        let files = if path.is_none() {
            self.lister_files(id, false).await
        } else {
            ListerFiles::Panel
        };
        self.view_target(id, target, files).await
    }

    /// Which files a Lister opened from panel `id` steps through: the marked ones while files
    /// are marked and `viewer.step_through_selection` is on, this one alone for `cursor_only`
    /// (Shift+F3) while files are marked, else the panel's (RC-100-91).
    pub(crate) async fn lister_files(&self, id: PanelId, cursor_only: bool) -> ListerFiles {
        let marked = !self.state().panel(id).selection.is_empty();
        if !marked {
            ListerFiles::Panel
        } else if cursor_only {
            ListerFiles::Single
        } else if self.setting_on(STEP_THROUGH_SELECTION_KEY).await {
            ListerFiles::Marked
        } else {
            ListerFiles::Panel
        }
    }

    /// `target` announced for the window's viewer, opened from panel `id`, `files` the ones N and
    /// P step through: `view.open`, and F3 (`file.open_external`) when no external viewer is
    /// set. A directory is refused.
    pub(crate) async fn view_target(
        &self,
        id: PanelId,
        target: VfsPath,
        files: ListerFiles,
    ) -> Result<Outcome, EngineError> {
        let entry = self
            .open_vfs(&target, self.announces_locked_vault())
            .await?
            .stat(&target.path)
            .await?;
        if entry.is_dir_like() {
            return Err(VfsError::IsADirectory { path: target.path }.into());
        }
        self.publish(EngineEvent::ViewFile {
            panel: id,
            path: target,
            files,
        });
        Ok(Outcome::Panel(self.snapshot(Some(id))))
    }

    /// `panel.toggle_quick_view`: announces that the Quick View fed by the panel's cursor is to
    /// be switched; which panel shows it is the window's state.
    pub(crate) fn toggle_quick_view(&self, panel: Option<PanelId>) -> Outcome {
        let snapshot = self.snapshot(panel);
        self.publish(EngineEvent::QuickView {
            panel: snapshot.panel,
        });
        Outcome::Panel(snapshot)
    }

    /// `path` opened for the viewer: its size read, nothing of its contents
    /// ([`FileWindow::open`]). The window's viewer and Quick View read every file through this.
    ///
    /// # Errors
    ///
    /// The backend is not available, or the path is a directory, a device or missing.
    pub async fn open_for_view(&self, path: &VfsPath) -> Result<FileWindow, EngineError> {
        let vfs = self.vfs(path)?;
        Ok(FileWindow::open(vfs, path.path.clone()).await?)
    }
}

#[cfg(test)]
#[path = "viewer_window_tests.rs"]
mod tests;
