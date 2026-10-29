//! The order of a run - renames that never overwrite another entry, swaps, rings and case-only
//! changes through temporary names - and the undo of a run or of the part carried out.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::RenameError;
use crate::names::FsRules;
use crate::plan::{Change, RenamePlan};

/// One rename inside the plan's directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameStep {
    /// The name before the step.
    pub from: String,
    /// The name after it.
    pub to: String,
}

/// The steps of a run, in the order they are carried out.
///
/// The undo log keeps exactly these steps, the temporary names included, so a run cancelled
/// between two steps is undone by [`Schedule::undo`] of the steps done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Schedule {
    /// The directory the steps rename in.
    pub dir: String,
    /// The steps; at no point does a step's `to` name an existing entry.
    pub steps: Vec<RenameStep>,
}

impl Schedule {
    /// The steps that reverse the first `done` steps (all of them when `done` is larger).
    #[must_use]
    pub fn undo(&self, done: usize) -> Vec<RenameStep> {
        undo_steps(&self.steps[..done.min(self.steps.len())])
    }
}

/// The steps that carry out `plan`, in time linear in its size.
///
/// An entry is renamed once the entry holding its new name has moved away, so the entries in
/// the way are renamed first. A chain of entries in the way that comes back to one of its own -
/// a swap, a ring, or a case-only change on a file system that ignores case - is broken by
/// parking that entry under a temporary name; it is renamed from there once its new name is
/// free. Temporary names are `.rc-rename-<tag>-<n>`, never a name the directory or the plan
/// uses; `tag` (the run's id) keeps two runs apart.
///
/// # Errors
///
/// [`RenameError::Conflicts`] when the plan has any conflict.
pub fn schedule(plan: &RenamePlan, tag: &str) -> Result<Schedule, RenameError> {
    let count = plan.conflict_count();
    if count > 0 {
        return Err(RenameError::Conflicts { count });
    }
    let fs = plan.fs;
    let mut moves: Vec<Move> = plan
        .items
        .iter()
        .filter(|item| item.change != Change::Unchanged)
        .map(|item| Move {
            from: item.old.clone(),
            to: item.new.clone(),
            state: State::Pending,
            parked: false,
        })
        .collect();
    let mut reserved: HashSet<String> = plan.taken.clone();
    reserved.extend(moves.iter().map(|step| fs.key(&step.to)));
    // The move not carried out yet that still holds a name, by the name's key.
    let mut holder: HashMap<String, usize> = moves
        .iter()
        .enumerate()
        .map(|(index, step)| (fs.key(&step.from), index))
        .collect();

    let mut steps = Vec::with_capacity(moves.len());
    let mut temporaries = 0_u64;
    let mut path: Vec<usize> = Vec::new();
    for start in 0..moves.len() {
        if moves[start].state != State::Pending {
            continue;
        }
        moves[start].state = State::OnPath;
        path.push(start);
        while let Some(&current) = path.last() {
            match holder.get(&fs.key(&moves[current].to)).copied() {
                None => {
                    let done = &mut moves[current];
                    holder.remove(&fs.key(&done.from));
                    done.state = State::Done;
                    steps.push(RenameStep {
                        from: std::mem::take(&mut done.from),
                        to: std::mem::take(&mut done.to),
                    });
                    path.pop();
                }
                Some(next) if moves[next].state == State::Pending => {
                    moves[next].state = State::OnPath;
                    path.push(next);
                }
                // The chain came back to `next` (to `current` itself for a case-only change).
                Some(next) => {
                    let parked = &mut moves[next];
                    if parked.parked {
                        return Err(RenameError::Unschedulable);
                    }
                    let temporary = loop {
                        temporaries += 1;
                        let name = temporary_name(tag, temporaries);
                        if !reserved.contains(&fs.key(&name)) {
                            break name;
                        }
                    };
                    reserved.insert(fs.key(&temporary));
                    holder.remove(&fs.key(&parked.from));
                    steps.push(RenameStep {
                        from: std::mem::replace(&mut parked.from, temporary.clone()),
                        to: temporary,
                    });
                    parked.parked = true;
                }
            }
        }
    }
    Ok(Schedule {
        dir: plan.dir.clone(),
        steps,
    })
}

/// One rename of the plan.
struct Move {
    from: String,
    to: String,
    state: State,
    /// Already moved to a temporary name; never parked twice.
    parked: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Not looked at yet.
    Pending,
    /// Waiting for the entry that holds its new name.
    OnPath,
    /// Carried out.
    Done,
}

/// How the temporary name of a parked entry starts; `rc_ops::temp_name` knows it among
/// rCommander's temporary names.
pub const TEMP_PREFIX: &str = ".rc-rename-";

fn temporary_name(tag: &str, number: u64) -> String {
    if tag.is_empty() {
        format!("{TEMP_PREFIX}{number}")
    } else {
        format!("{TEMP_PREFIX}{tag}-{number}")
    }
}

/// The steps that reverse `done`, steps carried out in this order: each reversed, last first.
#[must_use]
pub fn undo_steps(done: &[RenameStep]) -> Vec<RenameStep> {
    done.iter()
        .rev()
        .map(|step| RenameStep {
            from: step.to.clone(),
            to: step.from.clone(),
        })
        .collect()
}

/// A step of an undo that no longer fits the directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UndoMismatch {
    /// The step's index in the undo steps.
    pub index: usize,
    /// The step.
    pub step: RenameStep,
    /// What does not fit.
    pub problem: UndoProblem,
}

/// Why an undo step does not fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoProblem {
    /// The entry to rename back is gone or was renamed since.
    SourceMissing,
    /// The old name is taken again.
    TargetOccupied,
}

/// The undo steps that would fail against a directory holding `present`, walked in order as if
/// the fitting ones were carried out. Empty when the whole undo fits.
#[must_use]
pub fn check_undo(steps: &[RenameStep], present: &[String], fs: &FsRules) -> Vec<UndoMismatch> {
    let mut names: HashSet<String> = present.iter().map(|name| fs.key(name)).collect();
    let mut mismatches = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let (from, to) = (fs.key(&step.from), fs.key(&step.to));
        let problem = if !names.contains(&from) {
            UndoProblem::SourceMissing
        } else if to != from && names.contains(&to) {
            UndoProblem::TargetOccupied
        } else {
            names.remove(&from);
            names.insert(to);
            continue;
        };
        mismatches.push(UndoMismatch {
            index,
            step: step.clone(),
            problem,
        });
    }
    mismatches
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;
