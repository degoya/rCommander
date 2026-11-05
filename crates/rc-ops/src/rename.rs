//! Rename: the steps of a multi-rename, one after the other, inside one directory.
//!
//! The order comes made (`rc_rename::schedule`): no step's new name is taken when its turn
//! comes. Each step renames with [`Vfs::rename_no_replace`] anyway, because the directory may
//! have changed since the plan: a name taken meanwhile - also in the moment between a look and
//! the rename - fails the step with `already_exists` and is never overwritten (RC-080-64). A
//! failing step stops the job - skipping one would leave the steps after it renaming onto names
//! that are not free - and every step carried out is recorded in the job's journal before the
//! next one starts, so the undo log always knows how far a run got.
//!
//! A run that stops at a failing step while entries are under rCommander's temporary names
//! (`.rc-rename-*`, a swap's or a ring's middle) leaves nothing hidden without a word: with
//! [`crate::JobKind::rolling_back`] (`rename.undo_on_failure`) the job reverses its steps done
//! at once - the undo's own steps, `rc_rename::undo_steps`, each cleared in the journal - and
//! the snapshot's [`JobReport::RenameStopped`] says whether every name is as before; else, and
//! when the reversal fails too, it lists each parked entry with its temporary, old and new name,
//! and the error log carries the same list. A run without parked entries stops as Total
//! Commander's does, its undo by hand; a cancelled run stays as it is, with the same undo.
//!
//! A new name spelled `sub/name` (`rc_rename::SUBFOLDER_SEPARATOR`) moves the entry into a
//! subfolder of the directory, created first when it is missing, as Total Commander does; the
//! plan has kept such a name below the directory (`rc_rename::check_path`), and the queue holds
//! every step to that again when the job is submitted ([`check_steps`]).

use std::{path::Path, sync::Arc};

use rc_rename::{RenameStep, SUBFOLDER_SEPARATOR, TEMP_PREFIX, nested_path, undo_steps};
use rc_vfs::{Vfs, VfsPath};
use serde::{Deserialize, Serialize};

use crate::{
    Failure, JobReport, JobState, OpsError,
    runner::{Runner, Step, Stop, attempt},
};

/// An entry a stopped multi-rename left under one of its temporary names (RC-080-64).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParkedName {
    /// The temporary name it has now.
    pub temp: String,
    /// The name it had before the run.
    pub old: String,
    /// The name the run was to give it.
    pub new: String,
}

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

/// The entries under a temporary name once the first `done` of `steps` are carried out: each
/// step's new name that is a temporary one no later step done has taken off again, with the
/// name it came from and the one a step still to come was to give it.
pub(crate) fn parked(steps: &[RenameStep], done: usize) -> Vec<ParkedName> {
    let done = done.min(steps.len());
    steps[..done]
        .iter()
        .enumerate()
        .filter(|(at, step)| {
            step.to.starts_with(TEMP_PREFIX)
                && !steps[at + 1..done]
                    .iter()
                    .any(|later| later.from == step.to)
        })
        .map(|(_, step)| ParkedName {
            temp: step.to.clone(),
            old: step.from.clone(),
            new: steps[done..]
                .iter()
                .find(|later| later.from == step.to)
                .map_or_else(|| step.from.clone(), |later| later.to.clone()),
        })
        .collect()
}

/// Renames `from` to `to` in `dir`, never onto an existing entry; a `to` in a subfolder gets its
/// missing folders first.
async fn rename_new(vfs: &dyn Vfs, dir: &Path, from: &Path, to: &Path) -> Result<(), Step> {
    if let Some(folder) = to.parent().filter(|folder| *folder != dir) {
        vfs.mkdir(folder, true).await?;
    }
    Ok(vfs.rename_no_replace(from, to).await?)
}

impl Runner {
    pub(crate) async fn rename(
        &mut self,
        dir: &VfsPath,
        steps: &[RenameStep],
        rollback: bool,
    ) -> Result<(), Stop> {
        self.job
            .update(|snapshot| snapshot.state = JobState::Running);
        self.progress.files_total = u64::try_from(steps.len()).unwrap_or(u64::MAX);
        self.tick(true);
        let mut done = 0;
        let stopped = self.rename_steps(dir, steps, &mut done).await;
        // A cancel (or a skipped step) stays as it is: the undo log has every step done.
        let failure = match stopped {
            Err(Stop::Failed(failure)) => failure,
            other => return other,
        };
        if parked(steps, done).is_empty() {
            return Err(Stop::Failed(failure));
        }
        let left = if rollback {
            self.roll_back(dir, steps, done).await
        } else {
            done
        };
        let parked = parked(steps, left);
        if !parked.is_empty() {
            let list = parked
                .iter()
                .map(|name| format!("{} (was {}, to be {})", name.temp, name.old, name.new))
                .collect::<Vec<_>>()
                .join(", ");
            tracing::warn!(
                dir = %dir.path.display(),
                parked = %list,
                "a multi-rename stopped with entries under temporary names"
            );
        }
        let report = JobReport::RenameStopped {
            rolled_back: left == 0,
            parked,
        };
        self.job.update(|snapshot| snapshot.report = Some(report));
        Err(Stop::Failed(failure))
    }

    /// Carries out `steps` in order, counting each one renamed into `done`.
    async fn rename_steps(
        &mut self,
        dir: &VfsPath,
        steps: &[RenameStep],
        done: &mut usize,
    ) -> Result<(), Stop> {
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
            *done = index + 1;
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

    /// Reverses the first `done` of `steps`, last first, each cleared in the journal; stops at
    /// the first reversal that fails, which is a failure of the job beside the one that stopped
    /// it. How many of the steps are still in effect: 0 when all are reversed.
    async fn roll_back(&mut self, dir: &VfsPath, steps: &[RenameStep], done: usize) -> usize {
        let vfs = Arc::clone(&self.dst);
        let mut left = done;
        for back in undo_steps(&steps[..done]) {
            let (from, to) = (
                nested_path(&dir.path, &back.from),
                nested_path(&dir.path, &back.to),
            );
            if let Err(step) = rename_new(vfs.as_ref(), &dir.path, &from, &to).await {
                // A rename gives no stop; were there one, the steps left stay listed.
                if let Step::Fail(error) = step {
                    let failure = Failure::new(&dir.join(&back.from), &error);
                    self.job.update(|snapshot| snapshot.failures.push(failure));
                }
                return left;
            }
            if let Some(journal) = &self.journal
                && let Err(reason) = journal.undone(left - 1).await
            {
                // Renamed back but not recorded: the undo log follows the folder (its count is
                // corrected against the listing), so the reversal goes on.
                tracing::warn!(%reason, "a reversed rename step is not recorded");
            }
            left -= 1;
        }
        left
    }
}

#[cfg(test)]
#[path = "rename_tests.rs"]
mod tests;
