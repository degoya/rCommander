//! `view.open` (F3) and `panel.toggle_quick_view` (Ctrl+Q): the window's viewer is wanted. The
//! engine names the file and announces it; reading and drawing are the window's, through
//! [`Engine::open_for_view`] and the GUI-free model in [`crate::viewer`].

use rc_vfs::{VfsError, VfsPath};

use crate::{Engine, EngineError, EngineEvent, Outcome, PanelId, viewer::FileWindow};

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
        self.view_target(id, target).await
    }

    /// `target` announced for the window's viewer, opened from panel `id`: `view.open`, and F3
    /// (`file.open_external`) when no external viewer is set. A directory is refused.
    pub(crate) async fn view_target(
        &self,
        id: PanelId,
        target: VfsPath,
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
