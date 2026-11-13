//! Server files edited through a local copy (RC-110-01), as Total Commander does on FTP: F4,
//! Shift+F4, Enter and F3 with an external viewer on an SFTP, FTP or FTPS file fetch it as a job
//! of the queue into a folder of its own (`folder.rs`), hand the copy to its program and watch
//! it (`watch.rs`, `poll.rs`). A save goes back up as a job through a part file and a rename
//! (`upload.rs`): asked first while `remote_edit.upload` is `ask`, at once with `auto`; the
//! server's file is looked at first (`remote_edit.check_server_changes`), and its Unix mode is
//! given back afterwards. A file with several links or another owner is written in place, after
//! a question; a link is followed to the file it leads to (`open.rs`).
//!
//! At most [`MAX_REMOTE_EDITS`] copies are open at once. `remote_edit.list` lists them;
//! `upload`, `skip`, `close` and `window` are the person's alone - the upload is their decision.
//! Over MCP `file.open` on a server file gives only a read-only copy without a watch: an agent
//! sets up no way back to the server. Quitting removes the unchanged copies and keeps the
//! changed ones with their session files (`kept.rs`); the next start offers them.

use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};

use crate::recover::{LockOrRecover as _, LogPanic as _};
use crate::{Engine, EngineEvent, Outcome, VfsPath};

mod error;
pub(crate) mod folder;
mod kept;
mod launch;
mod open;
mod poll;
mod rules;
mod types;
mod upload;
mod watch;

pub use error::RemoteEditError;
pub(crate) use open::OpenRemote;
pub use types::{
    Baseline, RemoteEditChange, RemoteEditId, RemoteEditInfo, RemoteEditQuestion, RemoteEditState,
    Stamp, UploadChoice,
};

use watch::Watch;

/// The most server files open for editing at once.
pub const MAX_REMOTE_EDITS: usize = 32;

/// The edits of the run, and whether the copies are being watched.
#[derive(Default)]
pub(crate) struct RemoteEdits {
    desk: Mutex<Desk>,
}

#[derive(Default)]
struct Desk {
    next: u64,
    edits: BTreeMap<RemoteEditId, Edit>,
    /// The task that reads the copies' stamps runs.
    polling: bool,
    /// Where the folders go instead of the unpacked copies' root: the tests' own.
    root: Option<PathBuf>,
    /// How long a closed edit's write in place may stall instead of `upload::IN_PLACE_STALL`:
    /// the tests' own.
    stall: Option<std::time::Duration>,
}

/// One edited file.
#[derive(Debug, Clone)]
struct Edit {
    /// The server file as it was opened (a link stays the link).
    shown: VfsPath,
    /// The file an upload replaces: `shown`, or where its link leads.
    remote: VfsPath,
    server: String,
    name: String,
    /// The edit's folder and the copy in it.
    dir: PathBuf,
    file: PathBuf,
    baseline: Baseline,
    /// The copy as last fetched or uploaded.
    in_step: Option<Stamp>,
    watch: Watch,
    /// Uploads write into the server's file itself, after a question.
    in_place: bool,
    state: RemoteEditState,
    question: Option<RemoteEditQuestion>,
    /// An upload of this file runs; a save meanwhile waits for it (`again`).
    uploading: bool,
    again: bool,
    /// Cancelled when the edit is closed: a write in place that has stalled then ends
    /// (`upload.rs`).
    closed: tokio_util::sync::CancellationToken,
}

impl Edit {
    fn info(&self, id: RemoteEditId) -> RemoteEditInfo {
        RemoteEditInfo {
            id,
            name: self.name.clone(),
            server: self.server.clone(),
            path: self.shown.clone(),
            state: self.state.clone(),
            question: self.question,
        }
    }

    /// What the session file of this edit holds.
    fn session(&self) -> folder::Session {
        folder::Session::new(
            (self.shown.clone(), self.remote.clone()),
            self.server.clone(),
            self.name.clone(),
            self.baseline,
            self.in_step,
            self.in_place,
        )
    }
}

impl Engine {
    fn edits(&self) -> std::sync::MutexGuard<'_, Desk> {
        self.inner
            .remote_edits
            .desk
            .lock_or_recover("remote_edit.desk")
    }

    /// The root the edit folders go under; `None` without a folder of the user's own.
    fn edit_root(&self) -> Option<PathBuf> {
        self.edits().root.clone().or_else(crate::unpacked::root)
    }

    /// The server files being edited, oldest first; never their content.
    #[must_use]
    pub fn remote_edits(&self) -> Vec<RemoteEditInfo> {
        self.edits()
            .edits
            .iter()
            .map(|(id, edit)| edit.info(*id))
            .collect()
    }

    /// How many server files are being edited: the status bar's count.
    #[must_use]
    pub fn remote_edit_count(&self) -> usize {
        self.edits().edits.len()
    }

    /// The edits whose change is not on the server: what quitting names.
    #[must_use]
    pub fn unsaved_remote_edits(&self) -> Vec<RemoteEditInfo> {
        self.remote_edits()
            .into_iter()
            .filter(|edit| edit.state.unsaved() || edit.question == Some(RemoteEditQuestion::Kept))
            .collect()
    }

    /// `remote_edit.list`.
    pub(crate) fn list_remote_edits(&self) -> Outcome {
        Outcome::RemoteEdits {
            edits: self.remote_edits(),
        }
    }

    /// `remote_edit.window`: the window opens the list of server files being edited.
    pub(crate) fn remote_edit_window(&self) -> Outcome {
        self.publish(EngineEvent::RemoteEditWindow);
        self.list_remote_edits()
    }

    /// `remote_edit.skip`: "Not now", "Cancel" or "Later" - the question goes, the change stays
    /// in the copy; the next save asks again.
    pub(crate) fn skip_remote_edit(&self, id: RemoteEditId) -> Result<Outcome, crate::EngineError> {
        self.with_edit(id, |edit| edit.question = None)?;
        self.announce_edit(id, RemoteEditChange::State);
        Ok(self.list_remote_edits())
    }

    /// `remote_edit.close`: "Done" or "Discard" - the copy is no longer watched and goes with its
    /// folder. The window asks first when it holds a change.
    pub(crate) async fn close_remote_edit(
        &self,
        id: RemoteEditId,
    ) -> Result<Outcome, crate::EngineError> {
        let edit = self
            .edits()
            .edits
            .remove(&id)
            .ok_or(RemoteEditError::Unknown { id })?;
        edit.closed.cancel();
        let dir = edit.dir;
        let _ = tokio::task::spawn_blocking(move || folder::remove(&dir))
            .await
            .log_panic("remote_edit.close");
        self.announce_edit(id, RemoteEditChange::Closed);
        Ok(self.list_remote_edits())
    }

    /// Runs `change` on the edit `id`.
    fn with_edit<T>(
        &self,
        id: RemoteEditId,
        change: impl FnOnce(&mut Edit) -> T,
    ) -> Result<T, RemoteEditError> {
        let mut desk = self.edits();
        let edit = desk
            .edits
            .get_mut(&id)
            .ok_or(RemoteEditError::Unknown { id })?;
        Ok(change(edit))
    }

    fn announce_edit(&self, id: RemoteEditId, change: RemoteEditChange) {
        self.publish(EngineEvent::RemoteEdit { id, change });
    }
}

#[cfg(test)]
pub(crate) mod test_support;
