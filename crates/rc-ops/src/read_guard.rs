//! The read guard at run time (RC-080-42): a job an MCP call submitted or loaded reads no file
//! the guard keeps from MCP - a key, a credential file, rCommander's own data. The engine judges
//! the sources when it submits the job; this judges each file again right before the job opens
//! it, so a file that became one after the submission (a link turned, a key copied into the
//! folder) is not read either. A refused file is a failure of that item (`sensitive_path`,
//! `on_error` applies), and nothing of it is read.
//!
//! The queue is given the guard once ([`crate::Queue::set_read_guards`]); it marks the jobs
//! submitted while [`ReadGuards::applies`] (inside an MCP call) with
//! [`crate::JobOptions::read_guarded`], which a restart keeps. A job of the window is never
//! marked and runs without a guard.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::BackendId;

pub use rc_checksum::ReadGuard;

use crate::{
    JobSpec, OpsError,
    runner::{Runner, Step, Stop, attempt},
    walk::Item,
};

/// What the queue asks about reading: whether a job submitted now runs under the guard, and the
/// guard of one backend's files.
pub trait ReadGuards: Send + Sync {
    /// Whether a job submitted or loaded now is guarded: inside an MCP call.
    fn applies(&self) -> bool;
    /// The guard for the files of `backend`, the job's source.
    fn guard(&self, backend: &BackendId) -> ReadGuard;
}

/// Why a guarded job reads nothing when the queue was given no guard: it fails closed.
const NO_GUARD: &str = "no read guard is attached";

/// `spec` as the queue takes it now: guarded exactly when `guards` apply.
pub(crate) fn marked(mut spec: JobSpec, guards: Option<&Arc<dyn ReadGuards>>) -> JobSpec {
    spec.options.read_guarded = guards.is_some_and(|guards| guards.applies());
    spec
}

/// The guard of one guarded job: the queue's, or - when it was given none - one that refuses
/// everything.
#[derive(Clone)]
pub(crate) enum JobGuard {
    Guards(Arc<dyn ReadGuards>),
    Closed,
}

impl JobGuard {
    /// The guard of a job whose options are `read_guarded`; `None` for any other job.
    pub(crate) fn of(guarded: bool, guards: Option<&Arc<dyn ReadGuards>>) -> Option<Self> {
        guarded.then(|| guards.map_or(Self::Closed, |guards| Self::Guards(Arc::clone(guards))))
    }

    /// The guard for the files of `backend`.
    pub(crate) fn on(&self, backend: &BackendId) -> ReadGuard {
        match self {
            Self::Guards(guards) => guards.guard(backend),
            Self::Closed => Arc::new(|_: &Path| Some(NO_GUARD)),
        }
    }
}

impl Runner {
    /// Refuses `path` of the job's source when its guard names a reason.
    pub(crate) async fn admit(&self, path: &Path) -> Result<(), OpsError> {
        self.admit_on(&self.src_id, path).await
    }

    /// Refuses `path` on `backend` when the job's guard names a reason. The guard follows links
    /// and may probe the file - blocking calls - so it runs on the blocking pool.
    pub(crate) async fn admit_on(&self, backend: &BackendId, path: &Path) -> Result<(), OpsError> {
        let Some(guard) = self.read_guard.as_ref().map(|guard| guard.on(backend)) else {
            return Ok(());
        };
        let fallback = Arc::clone(&guard);
        let owned: PathBuf = path.to_path_buf();
        let judged = tokio::task::spawn_blocking(move || guard(&owned)).await;
        let reason = match judged {
            Ok(reason) => reason,
            // The blocking pool is shutting down: judged here.
            Err(_) => fallback(path),
        };
        match reason {
            Some(reason) => Err(OpsError::Sensitive {
                path: path.to_path_buf(),
                reason,
            }),
            None => Ok(()),
        }
    }

    /// The file items the guard admits, each refused one a failure of its own (`on_error`
    /// applies) counted as done; folders and links pass. For the jobs that read their files in
    /// one go (packing).
    pub(crate) async fn admitted(&mut self, items: Vec<Item>) -> Result<Vec<Item>, Stop> {
        if self.read_guard.is_none() {
            return Ok(items);
        }
        let mut kept = Vec::with_capacity(items.len());
        for item in items {
            if !item.is_file() {
                kept.push(item);
                continue;
            }
            let at = self.src_path(&item.path);
            if attempt!(self, &at, self.admit(&item.path).await.map_err(Step::from)).is_some() {
                kept.push(item);
            } else {
                self.count_done(1, item.entry.size);
            }
        }
        Ok(kept)
    }
}

#[cfg(test)]
#[path = "read_guard_tests.rs"]
mod tests;
