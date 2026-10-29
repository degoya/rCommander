//! Carrying a plan out: the actions a caller chose ([`SyncPlan::assign`]) and the `rc-ops` jobs
//! that run them ([`SyncPlan::jobs`]), with what `sync.apply` (`dirs_sync_apply`) is asked.

use std::{collections::HashMap, path::PathBuf};

use rc_ops::{ConflictChoice, DeleteMode, ErrorPolicy, JobKind, JobOptions, JobSpec};
use serde::{Deserialize, Serialize};

use super::{SyncAction, SyncOperation, SyncOptions, SyncPlan};
use crate::EngineError;

/// One row's action, named by its path below both roots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowAction {
    pub path: PathBuf,
    pub action: SyncAction,
}

/// How the jobs of a synchronization run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncRun {
    /// The plan's deletes may run; without this a plan that deletes runs nothing at all.
    pub deletes_confirmed: bool,
    /// Where deleted entries go.
    pub delete_mode: DeleteMode,
    /// The answer to every conflict of the copies (a newer or different file replaces the old
    /// one there); `None` asks for each, which only the window can answer.
    pub on_conflict: Option<ConflictChoice>,
    /// What a failing file does.
    pub on_error: ErrorPolicy,
    /// Compare BLAKE3 checksums of source and copy after each file.
    pub verify: bool,
}

/// The parameters of `sync.apply`: compare two trees again and run the chosen actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncApplyRequest {
    /// The left directory: absolute, or relative to the left panel's; the left panel's when
    /// `None`.
    pub left: Option<String>,
    /// The right directory, likewise against the right panel.
    pub right: Option<String>,
    pub options: SyncOptions,
    /// Start from the actions the mode proposes; otherwise every row starts with none, and only
    /// the rows in `actions` act.
    pub proposed: bool,
    /// Actions by path, over the start.
    pub actions: Vec<RowAction>,
    pub run: SyncRun,
}

impl Default for SyncApplyRequest {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            options: SyncOptions::default(),
            proposed: true,
            actions: Vec::new(),
            run: SyncRun::default(),
        }
    }
}

impl SyncPlan {
    /// Sets the rows' actions: the proposed ones or none (`proposed`), then each of `actions`.
    ///
    /// # Errors
    ///
    /// `sync_row_not_found` for a path the plan has no row for, `sync_action_invalid` for an
    /// action its row cannot take; the plan is then unchanged.
    pub fn assign(&mut self, proposed: bool, actions: &[RowAction]) -> Result<(), EngineError> {
        let chosen = {
            let rows: HashMap<&std::path::Path, usize> = self
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| (row.path.as_path(), index))
                .collect();
            let mut chosen = Vec::with_capacity(actions.len());
            for wanted in actions {
                let Some(&index) = rows.get(wanted.path.as_path()) else {
                    return Err(EngineError::SyncRowNotFound {
                        path: wanted.path.display().to_string(),
                    });
                };
                if !self.rows[index].allows(wanted.action) {
                    return Err(EngineError::SyncActionInvalid {
                        row: index,
                        action: wanted.action,
                    });
                }
                chosen.push((index, wanted.action));
            }
            chosen
        };
        if !proposed {
            for row in &mut self.rows {
                row.action = SyncAction::None;
            }
        }
        for (index, action) in chosen {
            self.rows[index].action = action;
        }
        Ok(())
    }

    /// The `rc-ops` jobs that carry out the rows' actions, in the order to submit them: the
    /// queue runs the jobs of one target backend one after the other. A directory to create is
    /// left out when a copy goes into it or below it, since a copy creates its target.
    ///
    /// # Errors
    ///
    /// `sync_deletes_unconfirmed` as [`SyncPlan::operations`]: no job at all.
    pub fn jobs(&self, run: &SyncRun) -> Result<Vec<JobSpec>, EngineError> {
        let operations = self.operations(run.deletes_confirmed)?;
        let filled: Vec<_> = operations
            .iter()
            .filter_map(|operation| match operation {
                SyncOperation::Copy { target, .. } => Some(target.clone()),
                _ => None,
            })
            .collect();
        let plain = JobOptions {
            on_error: run.on_error,
            ..JobOptions::default()
        };
        let copies = JobOptions {
            on_conflict: run.on_conflict,
            on_error: run.on_error,
            verify: run.verify,
            ..JobOptions::default()
        };
        let jobs = operations
            .into_iter()
            .filter_map(|operation| match operation {
                SyncOperation::Mkdir { path }
                    if filled.iter().any(|target| {
                        target.backend == path.backend && target.path.starts_with(&path.path)
                    }) =>
                {
                    None
                }
                SyncOperation::Mkdir { path } => Some(JobSpec {
                    kind: JobKind::Mkdir { path },
                    options: plain,
                }),
                SyncOperation::Copy {
                    sources, target, ..
                } => Some(JobSpec {
                    kind: JobKind::Copy { sources, target },
                    options: copies,
                }),
                SyncOperation::Delete { sources } => Some(JobSpec {
                    kind: JobKind::Delete {
                        sources,
                        mode: run.delete_mode,
                    },
                    options: plain,
                }),
            })
            .collect();
        Ok(jobs)
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
