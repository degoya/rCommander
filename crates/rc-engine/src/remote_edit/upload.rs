//! Uploading a copy: the server's file looked at first (`remote_edit.check_server_changes`), a
//! file with links or another owner asked about, then a job of the queue - the copy into the
//! server's folder through `<name>.rcpart` and a rename (RC-060-05) - or, after "Overwrite in
//! place", a write into the file itself. The Unix mode of the baseline is given back where the
//! backend has modes (the rename would leave the upload's own) - not after a write in place,
//! which keeps the file's mode and is chosen where the file is someone else's, whose mode the
//! server lets nobody else set (RC-110-25); a mode the server refuses is logged and fails no
//! upload. The stat after it is the new baseline. One upload of a file runs at a time: saves
//! meanwhile give one upload after it. A write in place outlives its edit's closing, unless it
//! has stalled.

use std::{
    io,
    path::Path,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use rc_ops::{ConflictChoice, JobKind, JobOptions, JobSpec};
use rc_vfs::{SetAttributes, VfsError, VfsPath, WriteMode, WriteStream};
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;

use super::{
    Baseline, RemoteEditChange, RemoteEditId, RemoteEditQuestion, RemoteEditState, UploadChoice,
    folder, open::clean, rules,
};
use crate::recover::LogPanic as _;
use crate::{
    Engine, EngineError, Outcome,
    settings::{REMOTE_EDIT_CHECK_KEY, REMOTE_EDIT_UPLOAD_KEY},
};

/// How long a write in place whose edit was closed may take no byte before it is let go.
pub(super) const IN_PLACE_STALL: Duration = Duration::from_secs(30);

/// A write stream that counts the bytes its backend took: what tells a slow write from a
/// stalled one.
struct Counting {
    inner: WriteStream,
    taken: Arc<AtomicU64>,
}

impl AsyncWrite for Counting {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let polled = Pin::new(&mut self.inner).poll_write(context, bytes);
        if let Poll::Ready(Ok(count)) = &polled {
            self.taken.fetch_add(*count as u64, Ordering::Relaxed);
        }
        polled
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

/// What a look at the server's file found.
enum Found {
    /// As it was fetched or last uploaded.
    Same,
    /// It asks this question.
    Ask(RemoteEditQuestion),
}

impl Engine {
    /// `remote_edit.upload`: uploads the copy of `id` as `choice` says, or asks first; with
    /// `always` the later saves go up without a question (`remote_edit.upload` = `auto`, as
    /// Total Commander's `AutoReUpload`). Answers at once; the upload's end arrives as
    /// [`crate::EngineEvent::RemoteEdit`].
    pub(crate) async fn upload_remote_edit(
        &self,
        id: RemoteEditId,
        choice: UploadChoice,
        always: bool,
    ) -> Result<Outcome, EngineError> {
        self.with_edit(id, |_| ())?;
        if always {
            let auto = serde_json::Value::from("auto");
            self.set_protected_setting(REMOTE_EDIT_UPLOAD_KEY, &auto)
                .await?;
        }
        self.start_upload(id, choice);
        Ok(self.list_remote_edits())
    }

    /// Starts the upload of `id`, unless one runs: then it follows that one.
    pub(super) fn start_upload(&self, id: RemoteEditId, choice: UploadChoice) {
        let starts = self.with_edit(id, |edit| {
            if edit.uploading {
                edit.again = true;
                return false;
            }
            edit.uploading = true;
            edit.question = None;
            true
        });
        if matches!(starts, Ok(true)) {
            let engine = self.clone();
            tokio::spawn(async move { engine.upload_loop(id, choice).await });
        }
    }

    /// The upload of `id`, and the one after it while saves came meanwhile.
    async fn upload_loop(self, id: RemoteEditId, mut choice: UploadChoice) {
        loop {
            match self.before_upload(id, choice).await {
                Ok(Found::Same) => {}
                Ok(Found::Ask(question)) => {
                    let conflict = !matches!(question, RemoteEditQuestion::InPlace);
                    self.stop(id, Ok(question), conflict);
                    return;
                }
                Err(error) => {
                    self.stop(id, Err(error.code().to_owned()), false);
                    return;
                }
            }
            if !self.upload_once(id, choice).await {
                return;
            }
            // A save came while it ran.
            if !self.uploads_without_asking().await {
                self.stop(id, Ok(RemoteEditQuestion::Upload), false);
                return;
            }
            let _ = self.with_edit(id, |edit| edit.state = RemoteEditState::Changed);
            choice = UploadChoice::Upload;
        }
    }

    /// What must be asked before the upload of `id` as `choice`: a changed or gone file on the
    /// server, then a file written in place.
    async fn before_upload(
        &self,
        id: RemoteEditId,
        choice: UploadChoice,
    ) -> Result<Found, EngineError> {
        let (remote, baseline, in_place) = self.with_edit(id, |edit| {
            (edit.remote.clone(), edit.baseline, edit.in_place)
        })?;
        if choice == UploadChoice::Upload && self.setting_on(REMOTE_EDIT_CHECK_KEY).await {
            self.open_server(&remote).await?;
            match self.vfs(&remote)?.stat(&remote.path).await {
                Ok(entry) if Baseline::of(&entry).stamp == baseline.stamp => {}
                Ok(_) => return Ok(Found::Ask(RemoteEditQuestion::ServerChanged)),
                Err(VfsError::NotFound { .. }) => {
                    return Ok(Found::Ask(RemoteEditQuestion::ServerGone));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let replaces = matches!(choice, UploadChoice::Upload | UploadChoice::Overwrite);
        if in_place && replaces {
            return Ok(Found::Ask(RemoteEditQuestion::InPlace));
        }
        Ok(Found::Same)
    }

    /// One upload of `id`; whether a save came meanwhile and the loop goes on.
    async fn upload_once(&self, id: RemoteEditId, choice: UploadChoice) -> bool {
        let Ok((file, remote, baseline, dir, closed)) = self.with_edit(id, |edit| {
            edit.state = RemoteEditState::Uploading;
            (
                edit.file.clone(),
                edit.remote.clone(),
                edit.baseline,
                edit.dir.clone(),
                edit.closed.clone(),
            )
        }) else {
            return false;
        };
        self.announce_edit(id, RemoteEditChange::State);
        let stall = self.edits().stall.unwrap_or(IN_PLACE_STALL);
        let read = file.clone();
        let stamp = tokio::task::spawn_blocking(move || rules::local_stamp(&read))
            .await
            .log_panic("remote_edit.stamp")
            .unwrap_or_default();
        let sent = match choice {
            UploadChoice::InPlace => self
                .write_in_place(&file, &remote, &closed, stall)
                .await
                .map_err(|error| error.code().to_owned()),
            UploadChoice::AsCopy => {
                self.upload_job(&file, &remote, ConflictChoice::Rename)
                    .await
            }
            UploadChoice::Upload | UploadChoice::Overwrite => {
                self.upload_job(&file, &remote, ConflictChoice::Overwrite)
                    .await
            }
        };
        // A copy beside the file leaves the file and its baseline as they were.
        let after = match sent {
            Ok(()) if choice == UploadChoice::AsCopy => Ok(None),
            Ok(()) => self
                .after_upload(&remote, baseline, choice != UploadChoice::InPlace)
                .await
                .map(Some)
                .map_err(|error| error.code().to_owned()),
            Err(code) => Err(code),
        };
        let outcome = self.with_edit(id, |edit| {
            edit.uploading = false;
            match &after {
                Ok(new) => {
                    if let Some(new) = new {
                        edit.baseline = *new;
                        edit.in_step = stamp;
                    }
                    edit.state = RemoteEditState::Uploaded {
                        at: SystemTime::now(),
                    };
                }
                Err(code) => {
                    edit.state = RemoteEditState::Failed { code: code.clone() };
                }
            }
            let again = std::mem::take(&mut edit.again);
            // The loop goes on with the next upload of this file.
            edit.uploading = again;
            (again, edit.session())
        });
        let Ok((again, session)) = outcome else {
            return false;
        };
        let _ = tokio::task::spawn_blocking(move || folder::write_session(&dir, &session))
            .await
            .log_panic("remote_edit.session");
        self.announce_edit(id, RemoteEditChange::State);
        again
    }

    /// Ends the uploads of `id` for now: with a question for the person, or with the failure's
    /// code. A conflict marks the edit so.
    fn stop(&self, id: RemoteEditId, why: Result<RemoteEditQuestion, String>, conflict: bool) {
        let asked = why.is_ok();
        let _ = self.with_edit(id, |edit| {
            edit.uploading = false;
            edit.again = false;
            match why {
                Ok(question) => {
                    edit.question = Some(question);
                    if conflict {
                        edit.state = RemoteEditState::Conflict;
                    } else if !matches!(edit.state, RemoteEditState::Conflict) {
                        edit.state = RemoteEditState::Changed;
                    }
                }
                Err(code) => edit.state = RemoteEditState::Failed { code },
            }
        });
        let change = if asked {
            RemoteEditChange::Asked
        } else {
            RemoteEditChange::State
        };
        self.announce_edit(id, change);
    }

    /// The copy `file` onto `remote` as a job of the queue, `on_conflict` deciding about the
    /// file there; the code of the failure.
    async fn upload_job(
        &self,
        file: &Path,
        remote: &VfsPath,
        on_conflict: ConflictChoice,
    ) -> Result<(), String> {
        let code = |error: EngineError| error.code().to_owned();
        self.open_server(remote).await.map_err(code)?;
        let spec = JobSpec {
            kind: JobKind::CopyAs {
                source: VfsPath::local(file),
                target: remote.clone(),
            },
            options: JobOptions {
                on_conflict: Some(on_conflict),
                keep_attributes: false,
                ..JobOptions::default()
            },
        };
        let done = self.run_job(spec).await.map_err(code)?;
        if clean(&done) {
            return Ok(());
        }
        // A job stopped without a failure of its own was cancelled in the queue.
        Err(done
            .failures
            .first()
            .map_or_else(|| "cancelled".to_owned(), |failure| failure.code.clone()))
    }

    /// Writes `file` into the server's file `remote` itself: links, owner and mode stay. A
    /// write whose edit is closed (`closed`: Done or Discard) still runs to its end - cut, it
    /// would leave half of a file that was truncated - unless it has stalled: closed and no
    /// byte taken for `stall`, it is let go instead of waited for (RC-110-26).
    async fn write_in_place(
        &self,
        file: &Path,
        remote: &VfsPath,
        closed: &CancellationToken,
        stall: Duration,
    ) -> Result<(), EngineError> {
        self.open_server(remote).await?;
        let vfs = self.vfs(remote)?;
        let io = |error: io::Error| VfsError::from_io(error, &remote.path);
        let taken = Arc::new(AtomicU64::new(0));
        let write = async {
            let mut reader = tokio::fs::File::open(file).await.map_err(io)?;
            let mut writer = Counting {
                inner: vfs.open_write(&remote.path, WriteMode::Truncate).await?,
                taken: Arc::clone(&taken),
            };
            tokio::io::copy(&mut reader, &mut writer)
                .await
                .map_err(io)?;
            writer.shutdown().await.map_err(io)
        };
        tokio::pin!(write);
        tokio::select! {
            written = &mut write => return Ok(written?),
            () = closed.cancelled() => {}
        }
        let mut seen = taken.load(Ordering::Relaxed);
        loop {
            tokio::select! {
                written = &mut write => return Ok(written?),
                () = tokio::time::sleep(stall) => {
                    let now = taken.load(Ordering::Relaxed);
                    if now == seen {
                        return Err(io(std::io::ErrorKind::TimedOut.into()).into());
                    }
                    seen = now;
                }
            }
        }
    }

    /// After an upload: the baseline's Unix mode given back where the backend has modes and
    /// the upload replaced the file (`give_mode`; a write in place kept it), and the server's
    /// file as it is now - the new baseline. A mode the server refuses costs the mode, not the
    /// upload: the bytes are there.
    async fn after_upload(
        &self,
        remote: &VfsPath,
        baseline: Baseline,
        give_mode: bool,
    ) -> Result<Baseline, EngineError> {
        let vfs = self.vfs(remote)?;
        if give_mode
            && let Some(mode) = baseline.mode
            && vfs.capabilities().unix_mode
        {
            let change = SetAttributes {
                mode: Some(mode),
                readonly: None,
            };
            if let Err(error) = vfs.set_attributes(&remote.path, change).await {
                tracing::warn!(
                    code = error.code(),
                    "an uploaded server file did not get its mode back: {error}"
                );
            }
        }
        let mut now = Baseline::of(&vfs.stat(&remote.path).await?);
        now.mode = now.mode.or(baseline.mode);
        Ok(now)
    }
}

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
