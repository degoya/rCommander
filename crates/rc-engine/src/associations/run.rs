//! The engine's side of the internal associations (RC-100-91): the rules as stored, Enter's turn
//! for a file (`file.open` asks [`Engine::open_by_rule`] before an archive opens as a folder and
//! before the system's association), `file.run_association` - a context menu entry -,
//! `file.associations` and what the MCP gate reads.
//!
//! A command runs as RC-100-86 runs an own command's (`user_commands/run.rs`), the file standing
//! in for the entry under the cursor: `%P` is its folder, `%N` its name, `%1` both, `%2` every
//! marked file of the panel - the marks count when the file is in the panel's own folder. A file
//! inside an archive goes as a read-only copy under the temporary folder, as Enter hands it to the
//! system; one on a server is not fetched (later).

use std::path::Path;

use rc_vfs::{BackendId, VfsError, VfsPath};
use serde_json::Value;

use super::{Associations, OPEN_VERB, is_archive_name, open_for, parse_rules};
use crate::{
    Engine, EngineError, EngineEvent, Outcome, PanelId,
    commands::resolve_path,
    settings::{
        ASSOCIATIONS_ALL_MATCHES_KEY, ASSOCIATIONS_ENABLED_KEY, FILE_ASSOCIATIONS_KEY,
        stored_setting,
    },
    user_commands::{AskSource, CommandRef, FileRun, UserCommandError},
};

/// The last part of `path`, as text.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

impl Engine {
    /// The rules and the two switches now: as stored, or none and the switches' defaults. A
    /// stored text that does not fit is no rule (`settings.rs` logs it).
    pub async fn file_associations(&self) -> Associations {
        let enabled = self.setting_on(ASSOCIATIONS_ENABLED_KEY).await;
        let all_matches = self.setting_on(ASSOCIATIONS_ALL_MATCHES_KEY).await;
        let stored = match self.db() {
            Ok(db) => stored_setting(db, FILE_ASSOCIATIONS_KEY).await,
            Err(_) => None,
        };
        let rules = stored
            .as_ref()
            .and_then(Value::as_str)
            .map(|text| parse_rules(text).unwrap_or_default())
            .unwrap_or_default();
        Associations {
            rules,
            enabled,
            all_matches,
        }
    }

    /// `file.associations`: the window opens the associations' window.
    pub(crate) fn associations_window(&self) -> Outcome {
        self.publish(EngineEvent::AssociationsWindow);
        Outcome::Panel(self.snapshot(None))
    }

    /// Enter's turn for the file `target` of panel `id` (`file.open`): the `open` of the first
    /// rule that takes it runs; `None` when none does - switched off, no rule, a file on a server.
    pub(crate) async fn open_by_rule(
        &self,
        id: PanelId,
        target: &VfsPath,
    ) -> Result<Option<Outcome>, EngineError> {
        if !matches!(target.backend, BackendId::Local | BackendId::Archive { .. }) {
            return Ok(None);
        }
        let associations = self.file_associations().await;
        if !associations.enabled {
            return Ok(None);
        }
        let name = file_name(&target.path);
        let Some(action) = open_for(&associations.rules, &name, is_archive_name(&name)).cloned()
        else {
            return Ok(None);
        };
        self.run_on_file(id, "file.open", target, (OPEN_VERB, &action), None)
            .await?;
        Ok(Some(Outcome::Panel(self.snapshot(Some(id)))))
    }

    /// `file.run_association`: the command `verb` of the association of `path` (resolved against
    /// the panel's location) or of the file under the cursor, for the panel; `params` in place of
    /// a program's own.
    pub(crate) async fn run_association(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
        verb: &str,
        params: Option<String>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (directory, cursor) = {
            let state = self.state();
            let panel = state.panel(id);
            (panel.path.clone(), panel.cursor_entry().cloned())
        };
        let target = match (path, cursor) {
            (Some(path), _) => {
                let target = resolve_path(&directory, path);
                let entry = self.vfs(&target)?.stat(&target.path).await?;
                if entry.is_dir_like() {
                    return Err(UserCommandError::NoAssociation.into());
                }
                target
            }
            (None, Some(entry)) if !entry.is_dir_like() => directory.join(&entry.name),
            (None, _) => return Err(UserCommandError::NoAssociation.into()),
        };
        let name = file_name(&target.path);
        let associations = self.file_associations().await;
        let action = associations
            .command_for(&name, verb)
            .cloned()
            .ok_or(UserCommandError::NoAssociation)?;
        self.run_on_file(id, "file.run_association", &target, (verb, &action), params)
            .await
    }

    /// The command `action` (named `verb`) for the file `target` of panel `id`: the file, or its
    /// copy out of an archive, is the source panel's folder and cursor for the placeholders.
    async fn run_on_file(
        &self,
        id: PanelId,
        command: &'static str,
        target: &VfsPath,
        (verb, action): (&str, &CommandRef),
        params: Option<String>,
    ) -> Result<Outcome, EngineError> {
        let file = match &target.backend {
            BackendId::Local => target.path.clone(),
            // A read-only copy, as Enter hands a file inside an archive to the system.
            BackendId::Archive { .. } => self.unpack_to_temp(target).await?,
            // On a server a file would have to be fetched first.
            _ => {
                return Err(VfsError::Unsupported {
                    path: target.path.clone(),
                    operation: "run an internal association's command",
                }
                .into());
            }
        };
        let name = file_name(&file);
        let folder = file
            .parent()
            .map(|folder| folder.to_string_lossy().into_owned());
        let mut context = self.param_context(id);
        // `%2` and `%S` are the panel's marks only where the file is in the panel's own folder.
        if context.source_dir != folder {
            context.source_marked.clear();
        }
        context.source_dir = folder;
        context.source_cursor = Some(name.clone());
        let file_run = FileRun {
            panel: id,
            id: command,
            context,
            source: AskSource::Association {
                path: target.path.to_string_lossy().into_owned(),
                verb: verb.to_owned(),
            },
            title: name,
        };
        self.run_for_file(file_run, action, params).await
    }

    /// Whether `file.open` of `path` (or of the entry under the cursor) runs an internal
    /// association whose `open` only a person may start: one that starts as administrator, asks
    /// for its parameters or runs a row stricter than `exec` (RC-100-91, the MCP gate).
    pub(crate) async fn open_needs_person(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
    ) -> bool {
        let name = match path {
            Some(path) => file_name(Path::new(path)),
            None => {
                let id = self.resolve(panel);
                let state = self.state();
                match state.panel(id).cursor_entry() {
                    Some(entry) if !entry.is_dir_like() => entry.name.clone(),
                    _ => return false,
                }
            }
        };
        if name.is_empty() {
            return false;
        }
        let associations = self.file_associations().await;
        if !associations.enabled {
            return false;
        }
        match open_for(&associations.rules, &name, is_archive_name(&name)) {
            Some(action) => self.action_needs_person(action, 0).await,
            None => false,
        }
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
