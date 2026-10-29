//! The archive commands (RC-030-02): `archive.list` (an archive's entries with what it says
//! about each) and `archive.test` (Alt+Shift+F9, every entry read through to check the format's
//! checksums), and the refusal of changes inside an archive.

use std::{collections::BTreeMap, path::PathBuf};

use rc_ops::{ErrorPolicy, JobKind, JobOptions, JobSpec};
use rc_vfs::{ArchiveItem, BackendId, Entry, VfsError, VfsPath};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use super::{ops::sources_of, panel::resolve_path};
use crate::{
    Engine, EngineError, Outcome, PanelId,
    archives::shown_path,
    settings::{ARCHIVE_ENTER_KEY, stored_setting},
    sort::{names_for, sort_entries},
};

/// Refuses a change inside an archive: archives are read-only in a panel.
pub(crate) fn refuse_archive_change(path: &VfsPath) -> Result<(), EngineError> {
    if matches!(path.backend, BackendId::Archive { .. }) {
        return Err(EngineError::ArchiveReadOnly);
    }
    Ok(())
}

/// The refusal of a file that is no archive this backend reads.
fn not_an_archive(path: &VfsPath) -> EngineError {
    EngineError::NotAnArchive {
        name: shown_path(path).display().to_string(),
    }
}

impl Engine {
    /// Opens the archive backend `path` lies in, so the operations queue knows it; any other
    /// backend is left as it is.
    pub(crate) fn open_backend(&self, path: &VfsPath) -> Result<(), EngineError> {
        if matches!(path.backend, BackendId::Archive { .. }) {
            self.archive_fs(&path.backend)?;
        }
        Ok(())
    }

    /// Whether Enter opens an archive in the panel (`archive.enter_opens`, on by default).
    pub(crate) async fn enter_opens_archives(&self) -> bool {
        let stored = match self.db() {
            Ok(db) => stored_setting(db, ARCHIVE_ENTER_KEY).await,
            Err(_) => None,
        };
        stored.as_ref().and_then(Value::as_bool).unwrap_or(true)
    }

    /// `archive.list`: the entries of a directory inside an archive in the panel's order,
    /// hidden ones included, each with what the archive says about it. `path` names the archive
    /// file; without it the archive the panel shows. `dir` is the directory inside (`/` the
    /// root; relative to the panel's directory when the panel is in that archive).
    pub(crate) async fn archive_list(
        &self,
        panel: Option<PanelId>,
        path: Option<&str>,
        dir: Option<&str>,
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (current, sort) = {
            let state = self.state();
            let panel = state.panel(id);
            (panel.path.clone(), panel.sort)
        };
        let start = match path {
            Some(path) => {
                let file = resolve_path(&current, path);
                self.archive_root(&file)
                    .await?
                    .ok_or_else(|| not_an_archive(&file))?
            }
            None => current,
        };
        let location = dir.map_or_else(|| start.clone(), |dir| resolve_path(&start, dir));
        if !matches!(location.backend, BackendId::Archive { .. }) {
            return Err(not_an_archive(&location));
        }
        let items = self
            .archive_fs(&location.backend)?
            .list_detailed(&location.path)
            .await?;
        let mut entries: Vec<Entry> = items.iter().map(|item| item.entry.clone()).collect();
        sort_entries(&mut entries, sort, names_for(&location));
        let mut by_name: BTreeMap<String, ArchiveItem> = items
            .into_iter()
            .map(|item| (item.entry.name.clone(), item))
            .collect();
        let entries = entries
            .iter()
            .filter_map(|entry| by_name.remove(&entry.name))
            .collect();
        Ok(Outcome::ArchiveListing {
            shown: shown_path(&location),
            path: location,
            entries,
        })
    }

    /// `archive.test`: one background job per archive that reads every entry to its end, so the
    /// format checks its CRCs and stream checksums. The archives are the paths named, else the
    /// archive the panel shows, else the selected files (or the one under the cursor) that are
    /// archives. Answers the queued jobs; their failures are the damaged and the encrypted
    /// entries.
    pub(crate) async fn archive_test(
        &self,
        panel: Option<PanelId>,
        paths: &[String],
    ) -> Result<Outcome, EngineError> {
        let id = self.resolve(panel);
        let (current, sources) = {
            let state = self.state();
            let panel = state.panel(id);
            let sources = sources_of(panel, paths).unwrap_or_default();
            (panel.path.clone(), sources)
        };
        let mut roots = Vec::new();
        if paths.is_empty() && matches!(current.backend, BackendId::Archive { .. }) {
            roots.push(VfsPath {
                backend: current.backend.clone(),
                path: PathBuf::from("/"),
            });
        } else {
            for file in &sources {
                match self.archive_root(file).await {
                    Ok(Some(root)) => roots.push(root),
                    // Among a selection, what is no archive is passed over.
                    Ok(None) | Err(_) if paths.is_empty() => {}
                    Ok(None) => return Err(not_an_archive(file)),
                    Err(error) => return Err(error),
                }
            }
        }
        if roots.is_empty() {
            return Err(not_an_archive(sources.first().unwrap_or(&current)));
        }
        let mut jobs = Vec::with_capacity(roots.len());
        for archive in roots {
            self.open_backend(&archive)?;
            let spec = JobSpec {
                kind: JobKind::TestArchive { archive },
                options: JobOptions {
                    on_error: ErrorPolicy::Skip,
                    ..JobOptions::default()
                },
            };
            jobs.push(self.inner.ops.submit(spec)?);
        }
        Ok(Outcome::Jobs { jobs })
    }

    /// The file `target` inside an archive, unpacked into a folder of its own under the system's
    /// temporary folder and marked read-only, for a program of the system to open. Nothing is
    /// written back into the archive.
    pub(crate) async fn unpack_to_temp(&self, target: &VfsPath) -> Result<PathBuf, EngineError> {
        let name = target
            .path
            .file_name()
            .ok_or_else(|| VfsError::InvalidPath {
                path: target.path.clone(),
                reason: "no file name",
            })?;
        // Removed at exit, or at a later start (`unpacked.rs`).
        let dir = crate::unpacked::new_folder()
            .await
            .map_err(|error| VfsError::from_io(error, &target.path))?;
        let file = dir.join(name);
        let mut reader = self.vfs(target)?.open_read(&target.path).await?;
        let written = async {
            let mut writer = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file)
                .await?;
            tokio::io::copy(&mut reader, &mut writer).await?;
            writer.flush().await?;
            let mut permissions = writer.metadata().await?.permissions();
            permissions.set_readonly(true);
            tokio::fs::set_permissions(&file, permissions).await
        }
        .await;
        if let Err(error) = written {
            // Nothing half unpacked is handed on or left behind.
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(VfsError::from_io(error, &target.path).into());
        }
        Ok(file)
    }
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
