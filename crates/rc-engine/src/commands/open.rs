//! `file.open`, what Enter does in Total Commander: a directory is shown in the panel, and so is
//! an archive (by its extension, while `archive.enter_opens` is on); any other entry is handed to
//! the system - a program starts, a document opens in the program associated with it
//! (`ShellExecuteW` on Windows, `open` on macOS, `xdg-open` elsewhere). A file inside an archive
//! is unpacked into a temporary folder first, read-only.

use std::path::Path;

use rc_vfs::{BackendId, Entry, EntryKind, VfsError, VfsPath};

use super::panel::resolve_path;
use crate::{
    Engine, EngineError, Outcome, PanelId, archives::opens_on_enter, mcp_questions::check_mcp_pin,
};

impl Engine {
    /// `file.open`: `path` (resolved against the panel's directory), or the entry under the
    /// cursor. A file answers the unchanged panel once the system took it. A panel an MCP call
    /// was asked about must still show the folder the question showed (`panel_changed`,
    /// RC-080-34).
    pub(crate) async fn open_file(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (directory, cursor) = {
            let state = self.state();
            let panel = state.panel(id);
            check_mcp_pin(id, &panel.path)?;
            (panel.path.clone(), panel.cursor_entry().cloned())
        };
        let (target, is_dir) = match path {
            Some(path) => {
                let target = resolve_path(&directory, path);
                let entry = self.vfs(&target)?.stat(&target.path).await?;
                let is_dir = entry.is_dir_like();
                (target, is_dir)
            }
            None => match cursor {
                Some(entry) => {
                    let target = directory.join(&entry.name);
                    let is_dir = self.leads_to_dir(&target, &entry).await?;
                    (target, is_dir)
                }
                None => return Ok(Outcome::Panel(self.snapshot(Some(id)))),
            },
        };
        if is_dir {
            // The path as it is: a name that is no UTF-8 would not survive a round trip through
            // text (RC-080-50).
            return self.show_path(id, target).await;
        }
        let name = target
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        // An archive opens in the panel, as a folder does (RC-030-02).
        if target.backend == BackendId::Local
            && opens_on_enter(&name)
            && self.enter_opens_archives().await
            && let Ok(Some(root)) = self.archive_root(&target).await
        {
            return self.show_path(id, root).await;
        }
        let file = match target.backend {
            BackendId::Local => target.path,
            // Inside an archive the system gets a read-only copy.
            BackendId::Archive { .. } => self.unpack_to_temp(&target).await?,
            // On a server a file would have to be fetched first.
            _ => {
                return Err(VfsError::Unsupported {
                    path: target.path,
                    operation: "open with the system",
                }
                .into());
            }
        };
        // ShellExecuteW may wait on the shell; never on the engine's caller.
        let launched = tokio::task::spawn_blocking(move || launch(&file)).await;
        if !matches!(launched, Ok(Ok(()))) {
            return Err(EngineError::OpenFailed { name });
        }
        Ok(Outcome::Panel(self.snapshot(Some(id))))
    }

    /// Whether the listed `entry` at `path` is a directory or a link to one. A link whose
    /// listing did not tell what it points at (FTP, RC-080-46) is looked at first.
    ///
    /// # Errors
    ///
    /// The look at the link failed.
    pub(crate) async fn leads_to_dir(
        &self,
        path: &VfsPath,
        entry: &Entry,
    ) -> Result<bool, EngineError> {
        let unresolved = matches!(
            entry.kind,
            EntryKind::Symlink {
                target_kind: None,
                ..
            }
        );
        if !unresolved {
            return Ok(entry.is_dir_like());
        }
        Ok(self.vfs(path)?.stat(&path.path).await?.is_dir_like())
    }
}

/// Hands `file` to the system without waiting for the program it starts.
#[cfg(not(test))]
fn launch(file: &Path) -> std::io::Result<()> {
    open::that_detached(file)
}

/// The tests start nothing: the file is recorded.
#[cfg(test)]
fn launch(file: &Path) -> std::io::Result<()> {
    tests::LAUNCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(file.to_path_buf());
    Ok(())
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
