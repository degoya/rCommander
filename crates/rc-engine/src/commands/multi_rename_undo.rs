//! `multi_rename.undo` and `multi_rename.next_step`.
//!
//! An undo reverses the latest run of the undo log as far as it got (`multi_rename.rs` has the
//! whole account). A run undone already leaves its rows with no mark; the undo then goes on to
//! the run before it (RC-100-96), so the steps of a chain (F5, the next step) are undone one
//! after the other, the latest first - at most [`MAX_UNDO_DEPTH`] runs back.
//!
//! The next step gives the names of a run's entries as they are after it: the names of the
//! sources in their order, each moved through the steps of the latest run that are marked done.
//! The dialog takes them as the sources of the next rule.

use std::collections::{BTreeSet, HashMap};

use rc_db::{LoggedStep, RenameRun};
use rc_rename::{RenameStep, SUBFOLDER_SEPARATOR, nested_path};
use rc_vfs::VfsPath;

use super::multi_rename::{Direction, Journal, fs_rules};
use crate::{Engine, EngineError, Outcome};

/// How many runs an undo looks back past runs undone already.
pub(super) const MAX_UNDO_DEPTH: usize = 32;

/// A logged run that an undo can reverse: its folder, what is there now, the steps.
struct Undoable {
    logged: RenameRun,
    dir: VfsPath,
    /// How many of its steps were carried out, by the marks and the folder.
    applied: usize,
    steps: Vec<RenameStep>,
    present: Vec<String>,
    fs: rc_rename::FsRules,
}

impl Engine {
    /// `multi_rename.undo`: the latest run with something carried out, reversed as far as it
    /// got.
    pub(crate) async fn multi_rename_undo(&self) -> Result<Outcome, EngineError> {
        let db = self.db()?.clone();
        let _claim = self.claim_rename()?;
        let mut logged = db
            .latest_rename_run()
            .await?
            .ok_or(EngineError::RenameNothingToUndo)?;
        let mut depth = 0;
        let found = loop {
            if !logged.is_complete() {
                return Err(EngineError::RenameUndoIncomplete);
            }
            let found = self.undoable(logged).await?;
            if found.applied > 0 {
                break found;
            }
            depth += 1;
            logged = match db.rename_run_before(&found.logged.run).await? {
                Some(before) if depth < MAX_UNDO_DEPTH => before,
                _ => return Err(EngineError::RenameNothingToUndo),
            };
        };
        let Undoable {
            logged,
            dir,
            applied,
            steps,
            present,
            fs,
        } = found;
        let undo = rc_rename::undo_steps(&steps[..applied]);
        let mismatches = rc_rename::check_undo(&undo, &present, &fs);
        if !mismatches.is_empty() {
            return Err(EngineError::RenameUndoMismatch { mismatches });
        }
        if applied != logged.marked() {
            // A crash between a rename and its mark: the log follows the folder.
            db.set_rename_marks(&logged.run, applied).await?;
        }
        let journal = Journal {
            db,
            run: logged.run.clone(),
            direction: Direction::Undo { applied },
        };
        self.submit_rename(logged.run, dir, undo, (journal, false))
            .await
    }

    /// How far `logged` got, read off its marks and its folder as it is now.
    async fn undoable(&self, logged: RenameRun) -> Result<Undoable, EngineError> {
        let dir: VfsPath =
            serde_json::from_str(&logged.dir).map_err(|_| EngineError::RenameUndoIncomplete)?;
        let vfs = self.vfs(&dir)?;
        let mut present: Vec<String> = vfs
            .list(&dir.path)
            .await?
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        // Names the run moved into subfolders are not in the listing: each is looked up.
        let nested: BTreeSet<&str> = logged
            .steps
            .iter()
            .flat_map(|step| [step.from.as_str(), step.to.as_str()])
            .filter(|name| name.contains(SUBFOLDER_SEPARATOR))
            .collect();
        for name in nested {
            if vfs.stat(&nested_path(&dir.path, name)).await.is_ok() {
                present.push(name.to_owned());
            }
        }
        let fs = fs_rules(&dir, vfs.capabilities());
        let steps: Vec<RenameStep> = logged
            .steps
            .iter()
            .map(|step| RenameStep {
                from: step.from.clone(),
                to: step.to.clone(),
            })
            .collect();
        let applied = rc_rename::applied_count(&steps, logged.marked(), &present, &fs);
        Ok(Undoable {
            logged,
            dir,
            applied,
            steps,
            present,
            fs,
        })
    }

    /// `multi_rename.next_step`: `names` as they are after the latest run - each moved through
    /// the run's steps marked done; a name it moved into a subfolder has left the folder and is
    /// left out. Without a run the names are as given.
    pub(crate) async fn multi_rename_next_step(
        &self,
        names: Vec<String>,
    ) -> Result<Outcome, EngineError> {
        let db = self.db()?.clone();
        let Some(logged) = db.latest_rename_run().await? else {
            return Ok(Outcome::RenameNextStep { names });
        };
        Ok(Outcome::RenameNextStep {
            names: after_run(names, &logged.steps),
        })
    }
}

/// `names` moved through the `steps` of a run that are marked done, in order; the ones that end
/// in a subfolder left out.
pub(super) fn after_run(mut names: Vec<String>, steps: &[LoggedStep]) -> Vec<String> {
    let mut at: HashMap<String, usize> = names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect();
    for step in steps.iter().take_while(|step| step.done) {
        if let Some(index) = at.remove(&step.from) {
            names[index].clone_from(&step.to);
            at.insert(step.to.clone(), index);
        }
    }
    names.retain(|name| !name.contains(SUBFOLDER_SEPARATOR));
    names
}

#[cfg(test)]
#[path = "multi_rename_undo_tests.rs"]
mod tests;
