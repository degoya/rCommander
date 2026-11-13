//! A copy or move onto a portable device while it runs (RC-110-16).
//!
//! A device takes one call at a time: while it takes a file, a listing of it waits behind the
//! transfer and ends at its time limit, and the file itself is not there until its last byte.
//! With `panels.device_transfer_hint` on (the default) the engine says so instead of leaving the
//! person with an old list:
//!
//! - `panel.refresh` (Ctrl+R) of a panel on a device that a job still writes to is refused as
//!   `device_busy`; the window reads the panels again when the job is over.
//! - A copy or move whose target file a job that has not ended writes already is refused as
//!   `target_in_transfer` before it is submitted - not asked about as a file that "already
//!   exists" once the first transfer has put it there.
//!
//! The window's hint in the panel's status line reads the same jobs ([`writing_to`]). Only
//! copies and moves count; a device mounted as a folder (gvfs on Linux) is on the local backend
//! and takes no part.

use rc_ops::{JobKind, JobSnapshot, OpsError};
use rc_vfs::{BackendId, VfsPath};

use crate::{Engine, EngineError, PanelId, settings::DEVICE_TRANSFER_HINT_KEY};

/// Where a copy or move writes when that is on a portable device: the folder it copies into, or
/// the file a copy under a new name makes.
#[must_use]
pub fn device_target(kind: &JobKind) -> Option<&VfsPath> {
    let target = match kind {
        JobKind::Copy { target, .. }
        | JobKind::Move { target, .. }
        | JobKind::CopyAs { target, .. }
        | JobKind::MoveAs { target, .. } => target,
        _ => return None,
    };
    matches!(target.backend, BackendId::Device { .. }).then_some(target)
}

/// The entries a copy or move makes on a portable device, one per source: its name in the
/// target folder, or the new name. Empty for every other job.
#[must_use]
pub fn device_files(kind: &JobKind) -> Vec<VfsPath> {
    let Some(target) = device_target(kind) else {
        return Vec::new();
    };
    match kind {
        JobKind::Copy { sources, .. } | JobKind::Move { sources, .. } => sources
            .iter()
            .filter_map(|source| source.path.file_name())
            .map(|name| target.join(name))
            .collect(),
        _ => vec![target.clone()],
    }
}

/// The first job of `jobs` that has not ended and writes onto the device `backend`; `None` for a
/// backend that is no device.
#[must_use]
pub fn writing_to<'a>(
    backend: &BackendId,
    jobs: impl IntoIterator<Item = &'a JobSnapshot>,
) -> Option<&'a JobSnapshot> {
    if !matches!(backend, BackendId::Device { .. }) {
        return None;
    }
    jobs.into_iter().find(|job| {
        !job.state.is_finished()
            && device_target(&job.kind).is_some_and(|target| target.backend == *backend)
    })
}

/// The first entry `new` would make on a device that a job of `jobs` which has not ended makes
/// too.
#[must_use]
pub fn in_transfer<'a>(
    new: &JobKind,
    jobs: impl IntoIterator<Item = &'a JobSnapshot>,
) -> Option<VfsPath> {
    let wanted = device_files(new);
    if wanted.is_empty() {
        return None;
    }
    jobs.into_iter()
        .filter(|job| !job.state.is_finished())
        .flat_map(|job| device_files(&job.kind))
        .find(|file| wanted.contains(file))
}

impl Engine {
    /// Refuses the jobs `kinds` as `target_in_transfer` when one would write a file on a device
    /// that a job which has not ended writes already. Nothing is asked of the device.
    pub(crate) async fn refuse_in_transfer<'a>(
        &self,
        kinds: impl IntoIterator<Item = &'a JobKind>,
    ) -> Result<(), EngineError> {
        let onto_device: Vec<&JobKind> = kinds
            .into_iter()
            .filter(|kind| device_target(kind).is_some())
            .collect();
        if onto_device.is_empty() || !self.setting_on(DEVICE_TRANSFER_HINT_KEY).await {
            return Ok(());
        }
        let jobs = self.inner.ops.jobs();
        match onto_device
            .into_iter()
            .find_map(|kind| in_transfer(kind, &jobs))
        {
            Some(file) => Err(OpsError::TargetInTransfer { path: file.path }.into()),
            None => Ok(()),
        }
    }

    /// Refuses to read panel `id` again as `device_busy` while a job writes onto the device it
    /// shows: the listing would wait behind the transfer and show the folder as it was.
    pub(crate) async fn refuse_busy_device(&self, id: PanelId) -> Result<(), EngineError> {
        let backend = self.state().panel(id).path.backend.clone();
        if !matches!(backend, BackendId::Device { .. })
            || !self.setting_on(DEVICE_TRANSFER_HINT_KEY).await
        {
            return Ok(());
        }
        let jobs = self.inner.ops.jobs();
        match writing_to(&backend, &jobs) {
            Some(_) => Err(OpsError::DeviceBusy.into()),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "device_transfer_tests.rs"]
mod tests;
