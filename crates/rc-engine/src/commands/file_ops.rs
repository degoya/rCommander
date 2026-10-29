//! What Total Commander's function keys need besides the `ops.*` jobs (RC-010-07).
//!
//! - `ops.dialog`: F5 to F8, Shift+F4 and the others ask the window for their dialog. Like
//!   `panel.drive_menu` it changes nothing and is announced as [`EngineEvent::FileDialog`]; the
//!   dialog then runs `ops.copy_to`, `ops.move_to`, `ops.delete`, `ops.mkdir` or
//!   `file.open_external`.
//! - `ops.copy_to` and `ops.move_to`: the target as typed in the F5 and F6 dialogs. A relative
//!   target is resolved against the source panel's directory, `sftp://<connection>/<path>` is
//!   on that server (RC-060-05); it is a directory to copy into
//!   when it is one, ends with a separator or a `*.*` mask, or when there are several sources -
//!   otherwise the one source gets it as its new path (copy under a new name, rename).
//! - `file.open_external`: F4 hands a file to the editor of the settings (`files.editor`), else
//!   Notepad or `$VISUAL`/`$EDITOR`; Shift+F4 first creates the file when it is missing. A file
//!   inside an archive goes as a read-only copy under the temporary folder (RC-030-02). F3 goes
//!   to an external viewer only when one is set (`files.viewer`, Total Commander's "use external
//!   viewer"), else to the internal viewer (`view.open`'s Lister, RC-020-07). Both settings are
//!   protected, so no agent picks the program that runs.

use std::path::{Path, is_separator};

use rc_ops::{AT_ONCE, JobKind, JobOptions, JobSpec, OpsError, QueueListId, check_name};
use rc_vfs::{BackendId, VfsError, VfsPath, WriteMode};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use super::{archive::refuse_archive_change, ops::sources_of, panel::resolve_path};
use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelId,
    settings::{EDITOR_KEY, VIEWER_KEY, stored_setting},
};

/// A dialog of the file operations, as the window shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileDialog {
    /// F5: copy into the other panel's directory.
    Copy,
    /// Shift+F5: copy in the same directory under a new name.
    CopyHere,
    /// F6: move into the other panel's directory, or rename.
    Move,
    /// Shift+F6: rename in place, in the list.
    Rename,
    /// F7.
    Mkdir,
    /// F8 and Delete: into the recycle bin.
    Delete,
    /// Shift+F8 and Shift+Delete: gone for good.
    DeletePermanent,
    /// Shift+F4: a new text file, opened in the editor.
    NewText,
}

/// The program `file.open_external` starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalProgram {
    /// F3: `files.viewer` when one is set, else the internal viewer.
    #[default]
    Viewer,
    /// F4: `files.editor`, else Notepad on Windows, `$VISUAL` or `$EDITOR` elsewhere.
    Editor,
}

/// `target` as typed, without a trailing `*.*` or `*` mask, and whether it names a directory to
/// copy into by its form: a mask or a trailing separator.
pub(crate) fn typed_target(target: &str) -> (&str, bool) {
    let trimmed = target.trim();
    let unmasked = trimmed
        .strip_suffix("*.*")
        .or_else(|| trimmed.strip_suffix('*'))
        .filter(|rest| rest.is_empty() || rest.ends_with(is_separator));
    match unmasked {
        Some(rest) => (rest, true),
        None => (trimmed, trimmed.ends_with(is_separator)),
    }
}

impl Engine {
    /// `ops.dialog`: the window opens the dialog for `panel` - F8's as the permanent delete's
    /// where the panel has no recycle bin and `delete.permanent_without_trash` is on
    /// (`recycle_bin.rs`).
    pub(crate) async fn file_dialog(&self, panel: Option<PanelId>, dialog: FileDialog) -> Outcome {
        let id = self.resolve(panel);
        let dialog = self.delete_dialog(id, dialog).await;
        self.publish(EngineEvent::FileDialog { panel: id, dialog });
        Outcome::Panel(self.snapshot(Some(id)))
    }

    /// `ops.copy_to` and `ops.move_to`: into the queue list `queue` (F2, RC-010-15), at once
    /// and outside the lists when `None` ([`AT_ONCE`]).
    pub(crate) async fn ops_transfer_to(
        &self,
        panel: Option<PanelId>,
        sources: &[String],
        target: &str,
        (options, queue): (JobOptions, Option<QueueListId>),
        moving: bool,
    ) -> Result<Outcome, EngineError> {
        let (typed, into_by_form) = typed_target(target);
        if typed.is_empty() {
            return Err(OpsError::InvalidName {
                name: target.to_owned(),
                reason: "empty",
            }
            .into());
        }
        let id = self.resolve(panel);
        self.remember_selection(id, sources);
        let (mut sources, current) = {
            let state = self.state();
            let from = state.panel(id);
            (sources_of(from, sources)?, from.path.clone())
        };
        // `sftp://<connection>/<path>` reaches a server (RC-060-05); anything else is read
        // against the panel's own directory.
        let target = self.locate(&current, typed).await?;
        // Archives are read-only: nothing goes into one, nothing moves out of one; a copy out
        // of one reads it through the backend the queue knows (RC-030-03).
        refuse_archive_change(&target)?;
        if moving {
            sources.iter().try_for_each(refuse_archive_change)?;
        }
        if let Some(first) = sources.first() {
            self.open_backend(first)?;
            self.open_server(first).await?;
        }
        self.open_server(&target).await?;
        self.apply_transfer_settings().await;
        // A folder renamed to another case (Shift+F6 `Docs` to `docs`) is no folder to move into.
        let into = into_by_form
            || sources.len() != 1
            || (!self.case_variant(&sources[0], &target)? && self.is_directory(&target).await?);
        let kind = match (into, moving, sources.pop()) {
            (false, false, Some(source)) => JobKind::CopyAs { source, target },
            (false, true, Some(source)) => JobKind::MoveAs { source, target },
            (_, moving, last) => {
                sources.extend(last);
                if moving {
                    JobKind::Move { sources, target }
                } else {
                    JobKind::Copy { sources, target }
                }
            }
        };
        // F2 names a list; OK runs the job at once, as in Total Commander (owner, 2026-10-18).
        let list = queue.unwrap_or(AT_ONCE);
        self.submit_items(JobSpec { kind, options }, list).await
    }

    /// Whether `target` is `source` in another case on a backend where case does not count.
    fn case_variant(&self, source: &VfsPath, target: &VfsPath) -> Result<bool, EngineError> {
        let lower = |path: &VfsPath| path.path.to_string_lossy().to_lowercase();
        Ok(source.backend == target.backend
            && source.path != target.path
            && lower(source) == lower(target)
            && !self.vfs(target)?.capabilities().case_sensitive)
    }

    /// Whether `path` is an existing directory; a missing entry is not.
    async fn is_directory(&self, path: &VfsPath) -> Result<bool, EngineError> {
        match self.vfs(path)?.stat(&path.path).await {
            Ok(entry) => Ok(entry.is_dir_like()),
            Err(VfsError::NotFound { .. }) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// `file.open_external`: `path` (resolved against the panel's directory), or the file under
    /// the cursor - a directory there opens nothing. With `create`, a missing `path` is made
    /// as an empty file first.
    pub(crate) async fn open_external(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
        program: ExternalProgram,
        create: bool,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (directory, cursor) = {
            let state = self.state();
            let panel = state.panel(id);
            (panel.path.clone(), panel.cursor_entry().cloned())
        };
        let target = match (path, cursor) {
            (Some(path), _) => resolve_path(&directory, path),
            (None, Some(entry)) if !entry.is_dir_like() => directory.join(&entry.name),
            (None, _) => return Ok(Outcome::Panel(self.snapshot(Some(id)))),
        };
        if path.is_some() {
            self.ensure_file(&target, create).await?;
        }
        match (program, self.external_program(program).await) {
            // TC's "use external viewer" is off: F3 is rCommander's own viewer.
            (ExternalProgram::Viewer, None) => self.view_internally(id, target).await,
            (_, chosen) => self.launch_external(id, target, chosen).await,
        }
    }

    /// F3 without an external viewer: the internal viewer (Lister), as `view.open` (RC-020-07).
    async fn view_internally(
        &self,
        panel: PanelId,
        target: VfsPath,
    ) -> Result<Outcome, EngineError> {
        self.view_target(panel, target).await
    }

    /// Hands `target` to `program`, or to the system's program for it, without waiting.
    async fn launch_external(
        &self,
        panel: PanelId,
        target: VfsPath,
        program: Option<String>,
    ) -> Result<Outcome, EngineError> {
        let name = target
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let file = match &target.backend {
            BackendId::Local => target.path,
            // A file inside an archive goes as a read-only copy under the temporary folder
            // (RC-030-02); nothing is written back into the archive.
            BackendId::Archive { .. } => self.unpack_to_temp(&target).await?,
            // On a server a file would have to be fetched first.
            _ => {
                return Err(VfsError::Unsupported {
                    path: target.path,
                    operation: "open in an external program",
                }
                .into());
            }
        };
        // The system's shell may wait; never on the engine's caller.
        let launched = tokio::task::spawn_blocking(move || launch(&file, program.as_deref())).await;
        if !matches!(launched, Ok(Ok(()))) {
            return Err(EngineError::OpenFailed { name });
        }
        Ok(Outcome::Panel(self.snapshot(Some(panel))))
    }

    /// `target` is a file: it exists, or with `create` it is made empty (its name checked like
    /// a typed one).
    async fn ensure_file(&self, target: &VfsPath, create: bool) -> Result<(), EngineError> {
        let vfs = self.vfs(target)?;
        match vfs.stat(&target.path).await {
            Ok(entry) if entry.is_dir_like() => Err(VfsError::IsADirectory {
                path: target.path.clone(),
            }
            .into()),
            Ok(_) => Ok(()),
            Err(VfsError::NotFound { .. }) if create => {
                let name = target
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                check_name(&name)?;
                let mut file = vfs.open_write(&target.path, WriteMode::CreateNew).await?;
                file.shutdown()
                    .await
                    .map_err(|error| VfsError::from_io(error, &target.path))?;
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The program of the setting for `program`, or its default; `None` is the system's
    /// program for the file.
    async fn external_program(&self, program: ExternalProgram) -> Option<String> {
        let key = match program {
            ExternalProgram::Viewer => VIEWER_KEY,
            ExternalProgram::Editor => EDITOR_KEY,
        };
        let stored = match self.db() {
            Ok(db) => stored_setting(db, key).await,
            Err(_) => None,
        };
        let configured = stored
            .as_ref()
            .and_then(|value| value.as_str())
            .filter(|path| !path.is_empty())
            .map(str::to_owned);
        match program {
            ExternalProgram::Viewer => configured,
            ExternalProgram::Editor => configured.or_else(default_editor),
        }
    }
}

/// Notepad on Windows; `$VISUAL` or `$EDITOR` elsewhere, else the system's program.
fn default_editor() -> Option<String> {
    if cfg!(windows) {
        return Some("notepad.exe".to_owned());
    }
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|program| !program.trim().is_empty())
}

/// Hands `file` to `program`, or to the system's program for it, without waiting. The `open`
/// crate passes the path as one argument and starts no console window.
#[cfg(not(test))]
fn launch(file: &Path, program: Option<&str>) -> std::io::Result<()> {
    match program {
        Some(program) => open::with_detached(file, program),
        None => open::that_detached(file),
    }
}

/// The tests start nothing: the file and the program are recorded.
#[cfg(test)]
fn launch(file: &Path, program: Option<&str>) -> std::io::Result<()> {
    tests::LAUNCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((file.to_path_buf(), program.map(str::to_owned)));
    Ok(())
}

#[cfg(test)]
#[path = "file_ops_tests.rs"]
mod tests;
