//! Transfers that survive a lost connection and a restart (RC-060-07): the engine's side of the
//! operations queue's hooks.
//!
//! The [`TransferStore`] is the table `transfer_jobs` of the database: a copy or move to or from
//! a saved connection, or from disk to disk (RC-080-04), is kept from its submission until it is
//! over (a quick connection's id means nothing after a restart, so its jobs are not kept). The [`Reconnect`] hook opens a server's backend again
//! ([`Engine::open_vfs`]) without announcing a locked vault: a job has no window to ask in.
//! [`Engine::restore_transfers`] puts the jobs the last run left back into the queue, paused. A
//! job of a batch - one of the jobs per source of one operation, RC-010-14 - is stored with its
//! batch (`"batch"` beside the spec's fields), and the jobs of one batch come back as siblings.
//!
//! A stored row is input like any other. One that does not read as a transfer on this machine or
//! to or from a saved connection, with plain absolute paths, is logged by its id and dropped -
//! never run. A store
//! that fails is logged too; it costs the resume after a restart, never the job.

use std::{
    io,
    path::{Component, Path},
    sync::{Arc, Weak},
};

use async_trait::async_trait;
use rc_db::StoredTransfer;
use rc_ops::{
    BatchId, FileMark, JobKind, JobSnapshot, JobSpec, OpsError, Reconnect, SavedJob, TransferStore,
    is_kept,
};
use rc_vfs::{BackendId, Vfs, VfsError, VfsPath};
use serde::Deserialize;

use crate::{Engine, EngineError, EngineEvent, Inner};

/// The hooks hold the engine weakly: the queue they live in is the engine's own.
struct Hooks(Weak<Inner>);

impl Hooks {
    fn engine(&self) -> Option<Engine> {
        self.0.upgrade().map(|inner| Engine { inner })
    }
}

impl Engine {
    /// Gives the operations queue its store and its reconnect hook.
    pub(crate) fn install_transfer_hooks(&self) {
        let hooks = Arc::new(Hooks(Arc::downgrade(&self.inner)));
        self.inner
            .ops
            .set_store(Arc::clone(&hooks) as Arc<dyn TransferStore>);
        self.inner.ops.set_reconnect(hooks as Arc<dyn Reconnect>);
    }

    /// Puts the transfer jobs the last run left in the middle back into the queue, paused and
    /// resumable: `ops.resume` continues one, `ops.cancel` discards it. The window calls it once
    /// at the start, after the database is attached; a headless `rcommander mcp` does not, so two
    /// processes never run one job.
    ///
    /// # Errors
    ///
    /// No database is attached, or it cannot be read.
    pub async fn restore_transfers(&self) -> Result<Vec<JobSnapshot>, EngineError> {
        let rows = self.db()?.transfers().await?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        self.apply_transfer_settings().await;
        // The rows of one batch together, in the order of their first row.
        let mut groups: Vec<(Option<BatchId>, Vec<SavedJob>)> = Vec::new();
        let mut dropped = Vec::new();
        for row in rows {
            let Some((saved, batch)) = read_row(&row) else {
                dropped.push(row.id);
                continue;
            };
            match groups
                .iter_mut()
                .find(|(kept, _)| batch.is_some() && *kept == batch)
            {
                Some((_, jobs)) => jobs.push(saved),
                None => groups.push((batch, vec![saved])),
            }
        }
        let mut restored = Vec::new();
        for (_, jobs) in groups {
            let records: Vec<i64> = jobs.iter().map(|saved| saved.record).collect();
            for (record, job) in records.into_iter().zip(self.inner.ops.restore_batch(jobs)) {
                match job {
                    Ok(job) => restored.push(job),
                    Err(_) => dropped.push(record),
                }
            }
        }
        for id in dropped {
            tracing::warn!(record = id, "a kept transfer job does not read; dropped");
            if let Err(error) = self.db()?.forget_transfer(id).await {
                tracing::warn!(record = id, %error, "a kept transfer job stays");
            }
        }
        if !restored.is_empty() {
            self.publish(EngineEvent::QueueChanged);
        }
        Ok(restored)
    }
}

/// What a stored row holds beside the spec: the batch of the job, when it is one of several.
#[derive(Deserialize)]
struct Kept {
    #[serde(default)]
    batch: Option<BatchId>,
}

/// The job of a stored row and its batch, when it is a transfer on this machine or to or from
/// saved connections, with plain absolute paths.
fn read_row(row: &StoredTransfer) -> Option<(SavedJob, Option<BatchId>)> {
    let spec: JobSpec = serde_json::from_str(&row.spec).ok()?;
    let Kept { batch } = serde_json::from_str(&row.spec).ok()?;
    if !is_kept(&spec.kind) || !locations(&spec.kind).iter().all(|path| plain(path)) {
        return None;
    }
    let mark = match &row.file {
        Some(file) => {
            let mark: FileMark = serde_json::from_str(file).ok()?;
            if !is_plain(&mark.source) || !is_plain(&mark.target) {
                return None;
            }
            Some(mark)
        }
        None => None,
    };
    let saved = SavedJob {
        record: row.id,
        spec,
        mark,
    };
    Some((saved, batch))
}

/// Every location a transfer names.
pub(crate) fn locations(kind: &JobKind) -> Vec<&VfsPath> {
    match kind {
        JobKind::Copy { sources, target } | JobKind::Move { sources, target } => {
            sources.iter().chain([target]).collect()
        }
        JobKind::CopyAs { source, target } | JobKind::MoveAs { source, target } => {
            vec![source, target]
        }
        _ => Vec::new(),
    }
}

/// A location a kept job may name: on this machine or a saved connection, never an archive or
/// a quick connection, with a plain absolute path.
pub(crate) fn plain(location: &VfsPath) -> bool {
    let backend = match location.backend {
        BackendId::Local => true,
        BackendId::Sftp { connection_id } | BackendId::Ftp { connection_id } => connection_id > 0,
        _ => false,
    };
    backend && is_plain(&location.path)
}

/// An absolute path without `..`.
fn is_plain(path: &Path) -> bool {
    let absolute = path.is_absolute() || path.starts_with("/");
    absolute
        && !path
            .components()
            .any(|component| component == Component::ParentDir)
}

#[async_trait]
impl TransferStore for Hooks {
    async fn save(&self, spec: &JobSpec) -> Option<i64> {
        self.save_in(spec, None).await
    }

    async fn save_in(&self, spec: &JobSpec, batch: Option<BatchId>) -> Option<i64> {
        if !locations(&spec.kind).iter().all(|path| plain(path)) {
            return None;
        }
        let engine = self.engine()?;
        let db = engine.db().ok()?;
        let mut value = serde_json::to_value(spec).ok()?;
        if let (Some(batch), Some(fields)) = (batch, value.as_object_mut()) {
            fields.insert("batch".to_owned(), batch.into());
        }
        let json = serde_json::to_string(&value).ok()?;
        match db.save_transfer(&json).await {
            Ok(id) => Some(id),
            Err(error) => {
                tracing::warn!(%error, "a transfer job is not kept for a resume after a restart");
                None
            }
        }
    }

    async fn mark(&self, record: i64, mark: &FileMark) {
        let Some(engine) = self.engine() else {
            return;
        };
        let (Ok(db), Ok(json)) = (engine.db(), serde_json::to_string(mark)) else {
            return;
        };
        if let Err(error) = db.mark_transfer(record, &json).await {
            tracing::warn!(record, %error, "the file of a kept transfer job is not recorded");
        }
    }

    async fn forget(&self, record: i64) {
        let Some(engine) = self.engine() else {
            return;
        };
        let Ok(db) = engine.db() else {
            return;
        };
        if let Err(error) = db.forget_transfer(record).await {
            tracing::warn!(record, %error, "a finished transfer job stays kept until the sweep");
        }
    }
}

#[async_trait]
impl Reconnect for Hooks {
    async fn reconnect(&self, backend: &BackendId) -> Result<Arc<dyn Vfs>, OpsError> {
        let engine = self.engine().ok_or(OpsError::BackendUnavailable)?;
        let location = VfsPath {
            backend: backend.clone(),
            path: "/".into(),
        };
        engine
            .open_vfs(&location, false)
            .await
            .map_err(|error| match error {
                EngineError::Ops(error) => error,
                EngineError::Vfs(error) => OpsError::from(error),
                // The reason by its code only: a session's error may quote the server.
                other => OpsError::from(VfsError::Io {
                    path: location.path.clone(),
                    source: io::Error::new(io::ErrorKind::NotConnected, other.code()),
                }),
            })
    }
}

#[cfg(test)]
#[path = "transfers_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transfers_crash_tests.rs"]
mod crash_tests;
