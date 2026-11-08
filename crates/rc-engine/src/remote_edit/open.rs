//! Opening a server file for editing: its name checked as a local copy's, a link followed to its
//! file, a program refused, the copy fetched as a job of the queue into a folder of its own,
//! watched, and handed to its program once the job is done.

use std::path::{Path, PathBuf};

use rc_ops::{AT_ONCE, ConflictChoice, JobKind, JobOptions, JobSnapshot, JobSpec, JobState};
use rc_vfs::{BackendId, Entry, EntryKind, VfsPath};

use super::{
    Baseline, Edit, MAX_REMOTE_EDITS, RemoteEditChange, RemoteEditError, RemoteEditId,
    RemoteEditState, folder, launch::launch, rules, watch::Watch,
};
use crate::recover::LogPanic as _;
use crate::{
    ConnectionId, Engine, EngineError, check_name, read_guard::from_mcp,
    settings::REMOTE_EDIT_PROGRAMS_KEY,
};

/// The most links followed to the file they lead to.
const MAX_HOPS: usize = 8;

/// Whether `backend` is a server whose files are edited through a copy.
pub(crate) fn is_server(backend: &BackendId) -> bool {
    matches!(backend, BackendId::Sftp { .. } | BackendId::Ftp { .. })
}

/// How a server file is to be opened.
#[derive(Debug, Clone, Default)]
pub(crate) struct OpenRemote {
    /// The program of the settings; `None` for the system's program for the file.
    pub program: Option<String>,
    /// Enter: the system decides how to open it, so a file that would run as a program is
    /// refused unless `remote_edit.open_programs` is on.
    pub system: bool,
    /// The listing's entry, when the file was under the cursor: its owner by name.
    pub listed: Option<Entry>,
}

impl Engine {
    /// Opens the server file `target` through a watched copy - or, inside an MCP call, through a
    /// read-only copy nobody watches.
    ///
    /// # Errors
    ///
    /// The name does not fit a local file, the file is no file or would run as a program, too
    /// many are open, the fetch failed, or the program did not start.
    pub(crate) async fn edit_remote(
        &self,
        target: VfsPath,
        how: OpenRemote,
    ) -> Result<(), EngineError> {
        let name = file_name(&target)?;
        // RC-080-43: a copy's name must be one a local folder takes (`con`, `a:b` are not).
        check_name(&name)?;
        self.open_server(&target).await?;
        let vfs = self.vfs(&target)?;
        let (remote, entry) = self.follow_links(&target, &name).await?;
        if how.system
            && rules::runs_here(&name, entry.attributes.mode)
            && !self.setting_on(REMOTE_EDIT_PROGRAMS_KEY).await
        {
            return Err(RemoteEditError::Program { name }.into());
        }
        if from_mcp() {
            // An agent sets up no way back to the server: a read-only copy, unwatched.
            let file = self.unpack_to_temp(&remote).await?;
            return start(file, how.program, name).await;
        }
        if self.remote_edit_count() >= MAX_REMOTE_EDITS {
            return Err(RemoteEditError::TooMany.into());
        }
        let (server, user) = self.server_of(&target.backend).await;
        let details = vfs.details(&remote.path).await.unwrap_or_default();
        let owner = match &how.listed {
            Some(listed) if remote == target && listed.owner.is_some() => listed,
            _ => &entry,
        };
        let in_place = rules::needs_in_place(owner, &details, user.as_deref());
        let file = self.fetch_copy(&remote, &name).await?;
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let marked = file.clone();
        let in_step = tokio::task::spawn_blocking(move || {
            rules::mark_from_internet(&marked);
            rules::local_stamp(&marked)
        })
        .await
        .log_panic("remote_edit.stamp")
        .unwrap_or_default();
        let edit = Edit {
            shown: target,
            remote,
            server,
            name: name.clone(),
            dir,
            file: file.clone(),
            baseline: Baseline::of(&entry),
            in_step,
            watch: Watch::new(in_step),
            in_place,
            state: RemoteEditState::Unchanged,
            question: None,
            uploading: false,
            again: false,
        };
        let id = self.adopt_edit(edit).await?;
        self.announce_edit(id, RemoteEditChange::Opened);
        start(file, how.program, name).await
    }

    /// The file `target` leads to and its entry, through at most [`MAX_HOPS`] links.
    async fn follow_links(
        &self,
        target: &VfsPath,
        name: &str,
    ) -> Result<(VfsPath, Entry), EngineError> {
        let vfs = self.vfs(target)?;
        let mut remote = target.clone();
        let mut entry = vfs.stat(&remote.path).await?;
        for _ in 0..MAX_HOPS {
            let EntryKind::Symlink { target: to, .. } = &entry.kind else {
                break;
            };
            remote.path = rules::link_destination(&remote.path, to);
            entry = vfs.stat(&remote.path).await?;
        }
        if !matches!(entry.kind, EntryKind::File) {
            return Err(RemoteEditError::NotAFile {
                name: name.to_owned(),
            }
            .into());
        }
        Ok((remote, entry))
    }

    /// The connection's name and user for the backend `backend`.
    async fn server_of(&self, backend: &BackendId) -> (String, Option<String>) {
        let id = match backend {
            BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id } => {
                ConnectionId(*connection_id)
            }
            _ => return (String::new(), None),
        };
        if let Ok(db) = self.db()
            && let Ok(connection) = db.connection(id).await
        {
            return (connection.name, connection.username);
        }
        (format!("#{}", id.0), None)
    }

    /// Fetches `remote` as a job of the queue into a new edit folder; the copy's path. Nothing
    /// fetched in part is left behind.
    async fn fetch_copy(&self, remote: &VfsPath, name: &str) -> Result<PathBuf, EngineError> {
        let root = self.edit_root().ok_or_else(|| EngineError::OpenFailed {
            name: name.to_owned(),
        })?;
        let dir = tokio::task::spawn_blocking(move || folder::create(&root))
            .await
            .log_panic("remote_edit.folder")
            .map_err(std::io::Error::other)
            .and_then(|made| made)
            .map_err(|error| rc_vfs::VfsError::from_io(error, &remote.path))?;
        let file = dir.join(name);
        let spec = JobSpec {
            kind: JobKind::CopyAs {
                source: remote.clone(),
                target: VfsPath::local(&file),
            },
            // The copy is the person's to change: no read-only flag or mode from the server.
            options: JobOptions {
                on_conflict: Some(ConflictChoice::Overwrite),
                keep_attributes: false,
                ..JobOptions::default()
            },
        };
        let fetched = self.run_job(spec).await;
        if !fetched.as_ref().is_ok_and(clean) {
            let gone = dir.clone();
            let _ = tokio::task::spawn_blocking(move || folder::remove(&gone))
                .await
                .log_panic("remote_edit.discard");
            fetched?;
            return Err(EngineError::OpenFailed {
                name: name.to_owned(),
            });
        }
        Ok(file)
    }

    /// Runs `spec` as a job of the queue at once and waits for its end.
    pub(super) async fn run_job(&self, spec: JobSpec) -> Result<JobSnapshot, EngineError> {
        let mut jobs = self.inner.ops.submit_batch(vec![spec], AT_ONCE)?;
        let Some(job) = jobs.pop() else {
            return Err(rc_ops::OpsError::NoSources.into());
        };
        Ok(self.inner.ops.wait(job.id).await?)
    }

    /// Takes `edit` into the desk, writes its session file and starts the watch; its id.
    pub(super) async fn adopt_edit(&self, edit: Edit) -> Result<RemoteEditId, EngineError> {
        let dir = edit.dir.clone();
        let session = edit.session();
        let taken = {
            let mut desk = self.edits();
            (desk.edits.len() < MAX_REMOTE_EDITS).then(|| {
                desk.next += 1;
                let id = RemoteEditId(desk.next);
                desk.edits.insert(id, edit);
                id
            })
        };
        let Some(id) = taken else {
            let _ = tokio::task::spawn_blocking(move || folder::remove(&dir))
                .await
                .log_panic("remote_edit.discard");
            return Err(RemoteEditError::TooMany.into());
        };
        // Without a session file the copy is still edited; only a kept change could not be
        // offered at the next start.
        let written = tokio::task::spawn_blocking(move || folder::write_session(&dir, &session))
            .await
            .log_panic("remote_edit.session");
        if !matches!(written, Ok(Ok(()))) {
            tracing::warn!("the session file of a server file being edited was not written");
        }
        self.ensure_polling();
        Ok(id)
    }
}

/// Whether the job `snapshot` went through: completed, nothing failed or skipped.
pub(super) fn clean(snapshot: &JobSnapshot) -> bool {
    snapshot.state == JobState::Completed && snapshot.failures.is_empty() && snapshot.skipped == 0
}

/// The file name of `target`.
fn file_name(target: &VfsPath) -> Result<String, EngineError> {
    target
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| {
            rc_vfs::VfsError::InvalidPath {
                path: target.path.clone(),
                reason: "no file name",
            }
            .into()
        })
}

/// Hands `file` to `program`, or to the system's program for it, off the async threads.
async fn start(file: PathBuf, program: Option<String>, name: String) -> Result<(), EngineError> {
    let launched = tokio::task::spawn_blocking(move || launch(&file, program.as_deref()))
        .await
        .log_panic("remote_edit.launch");
    if matches!(launched, Ok(Ok(()))) {
        Ok(())
    } else {
        Err(EngineError::OpenFailed { name })
    }
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
