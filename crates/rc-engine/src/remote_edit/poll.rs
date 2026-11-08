//! The task that watches the copies: every [`POLL`] it reads each copy's stamp off the async
//! threads and feeds it to the copy's [`super::Watch`]; a save seen goes on as
//! `remote_edit.upload` says - a question with `ask`, an upload with `auto`, after the running
//! upload of the same file when one runs. The task ends with the last edit and starts again
//! with the next.

use std::{path::PathBuf, sync::Arc};

use tokio::time::MissedTickBehavior;

use super::{
    RemoteEditChange, RemoteEditId, RemoteEditQuestion, RemoteEditState, UploadChoice, rules,
    watch::POLL,
};
use crate::recover::LogPanic as _;
use crate::{Engine, settings::REMOTE_EDIT_UPLOAD_KEY};

impl Engine {
    /// Starts the watch of the copies unless it runs.
    pub(super) fn ensure_polling(&self) {
        {
            let mut desk = self.edits();
            if desk.polling {
                return;
            }
            desk.polling = true;
        }
        let engine = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(POLL);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(inner) = engine.upgrade() else {
                    return;
                };
                if !(Engine { inner }).poll_copies().await {
                    return;
                }
            }
        });
    }

    /// One look at every copy; `false` once there is none (the watch then ends).
    pub(super) async fn poll_copies(&self) -> bool {
        let files: Vec<(RemoteEditId, PathBuf)> = {
            let mut desk = self.edits();
            if desk.edits.is_empty() {
                desk.polling = false;
                return false;
            }
            desk.edits
                .iter()
                .map(|(id, edit)| (*id, edit.file.clone()))
                .collect()
        };
        let stamps = tokio::task::spawn_blocking(move || {
            files
                .into_iter()
                .map(|(id, file)| (id, rules::local_stamp(&file)))
                .collect::<Vec<_>>()
        })
        .await
        .log_panic("remote_edit.poll")
        .unwrap_or_default();
        let saved: Vec<RemoteEditId> = {
            let mut desk = self.edits();
            stamps
                .into_iter()
                .filter(|(id, stamp)| {
                    desk.edits
                        .get_mut(id)
                        .is_some_and(|edit| edit.watch.observe(*stamp))
                })
                .map(|(id, _)| id)
                .collect()
        };
        for id in saved {
            self.on_saved(id).await;
        }
        true
    }

    /// A save of the copy of `id` was seen.
    pub(super) async fn on_saved(&self, id: RemoteEditId) {
        let auto = self.uploads_without_asking().await;
        let next = self.with_edit(id, |edit| {
            if edit.uploading {
                // After the running upload, once, however many saves came meanwhile.
                edit.again = true;
                return None;
            }
            edit.state = RemoteEditState::Changed;
            if auto {
                return Some(true);
            }
            // One question for the saves until it is answered.
            if edit.question.is_none() {
                edit.question = Some(RemoteEditQuestion::Upload);
                return Some(false);
            }
            None
        });
        match next {
            Ok(Some(true)) => self.start_upload(id, UploadChoice::Upload),
            Ok(Some(false)) => self.announce_edit(id, RemoteEditChange::Asked),
            Ok(None) => self.announce_edit(id, RemoteEditChange::State),
            Err(_) => {}
        }
    }

    /// Whether `remote_edit.upload` is `auto`.
    pub(super) async fn uploads_without_asking(&self) -> bool {
        let stored = match self.db() {
            Ok(db) => crate::settings::stored_setting(db, REMOTE_EDIT_UPLOAD_KEY).await,
            Err(_) => None,
        };
        stored.as_ref().and_then(serde_json::Value::as_str) == Some("auto")
    }
}
