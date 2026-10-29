//! Rename: the steps of a multi-rename, one after the other, inside one directory.
//!
//! The order comes made (`rc_rename::schedule`): no step's new name is taken when its turn
//! comes. Each step checks that anyway, because `Vfs::rename` replaces an existing file and the
//! directory may have changed since the plan; a taken name fails the step with
//! `already_exists`. A failing step stops the job - skipping one would leave the steps after it
//! renaming onto names that are not free - and every step carried out is recorded in the
//! job's journal before the next one starts, so the undo log always knows how far a run got.
//!
//! A new name spelled `sub/name` (`rc_rename::SUBFOLDER_SEPARATOR`) moves the entry into a
//! subfolder of the directory, created first when it is missing, as Total Commander does; the
//! plan has kept such a name below the directory (`rc_rename::check_path`), and the queue holds
//! every step to that again when the job is submitted ([`check_steps`]).

use std::{path::Path, sync::Arc};

use rc_rename::{RenameStep, SUBFOLDER_SEPARATOR, nested_path};
use rc_vfs::{Vfs, VfsError, VfsPath};

use crate::{
    Failure, JobState, OpsError,
    runner::{Runner, Step, Stop, attempt},
};

/// Accepts the steps of a rename job: each old and new name is one name, or a nested one whose
/// every part is one plain name, so `nested_path` keeps it below the directory - no `..`, no
/// root, drive or share, no empty part, on Windows no `\` or `:` inside a part. Only the
/// containment is held here: the naming rules are the plan's, and an undo goes back to names
/// the directory held, which those rules may refuse.
///
/// # Errors
///
/// `invalid_name` for the first name that leaves the directory.
pub(crate) fn check_steps(steps: &[RenameStep]) -> Result<(), OpsError> {
    let below = |name: &str| {
        name.split(SUBFOLDER_SEPARATOR)
            .try_for_each(crate::name::check_listed)
            .map_err(|_| OpsError::InvalidName {
                name: name.to_owned(),
                reason: "a path that is not plain names",
            })
    };
    steps
        .iter()
        .try_for_each(|step| below(&step.from).and_then(|()| below(&step.to)))
}

/// Renames `from` to `to` in `dir`, refusing a `to` that exists; a `to` in a subfolder gets its
/// missing folders first.
async fn rename_new(vfs: &dyn Vfs, dir: &Path, from: &Path, to: &Path) -> Result<(), Step> {
    match vfs.stat(to).await {
        Ok(_) => Err(VfsError::AlreadyExists {
            path: to.to_path_buf(),
        }
        .into()),
        Err(VfsError::NotFound { .. }) => {
            if let Some(folder) = to.parent().filter(|folder| *folder != dir) {
                vfs.mkdir(folder, true).await?;
            }
            Ok(vfs.rename(from, to).await?)
        }
        Err(error) => Err(error.into()),
    }
}

impl Runner {
    pub(crate) async fn rename(&mut self, dir: &VfsPath, steps: &[RenameStep]) -> Result<(), Stop> {
        self.job
            .update(|snapshot| snapshot.state = JobState::Running);
        self.progress.files_total = u64::try_from(steps.len()).unwrap_or(u64::MAX);
        self.tick(true);
        let vfs = Arc::clone(&self.dst);
        for (index, step) in steps.iter().enumerate() {
            self.checkpoint().await?;
            let at = dir.join(&step.from);
            self.progress.current = Some(at.clone());
            let (from, to) = (
                nested_path(&dir.path, &step.from),
                nested_path(&dir.path, &step.to),
            );
            let renamed = attempt!(
                self,
                &at,
                rename_new(vfs.as_ref(), &dir.path, &from, &to).await
            );
            if renamed.is_none() {
                // Skipped by an answer: the steps after this one would go wrong, so the run ends
                // here; its failure is in the snapshot and the steps done are in the journal.
                return Err(Stop::Cancelled);
            }
            if let Some(journal) = &self.journal
                && let Err(reason) = journal.done(index).await
            {
                let error = OpsError::JournalFailed { reason };
                return Err(Stop::Failed(Failure::new(&dir.join(&step.to), &error)));
            }
            self.count_done(1, 0);
        }
        Ok(())
    }
}
