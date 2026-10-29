//! What the undo log keeps per run, the log's bounds, and how far a logged run got.
//!
//! The log is the table `rename_log` in `rc-db` (RC-020-01), one row per step, listed in
//! `rc_db::RETAINED_TABLES` with the limits below and pruned by the shared sweep (RC-010-11),
//! oldest runs first. The engine writes every step of a run before the first is carried out and
//! marks each one once it is; an undo reverses the latest run, so only recent runs are worth
//! keeping.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::names::FsRules;
use crate::schedule::RenameStep;

/// The most steps (rows) the undo log keeps over all its runs.
pub const UNDO_LOG_MAX_STEPS: u32 = 100_000;

/// The oldest run the undo log keeps, in days.
pub const UNDO_LOG_MAX_AGE_DAYS: u32 = 30;

/// One run as the undo log keeps it; [`crate::undo_steps`] of `steps` reverses it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoRecord {
    /// The run's tag, the one in its temporary names.
    pub tag: String,
    /// The directory the run renamed in.
    pub dir: String,
    /// When the run started.
    pub started: DateTime<Utc>,
    /// The steps carried out, in order: all of them, or those done before a cancel.
    pub steps: Vec<RenameStep>,
}

/// How many of `steps` are carried out in a directory holding `present`, when the log marks the
/// first `marked` as done.
///
/// A run marks each step after carrying it out and an undo unmarks each step after reversing
/// it, so after a crash the step at the boundary can be carried out without its mark (one more
/// than `marked`) or reversed with its mark still set (one fewer). Before a step its `from`
/// exists and its `to` does not, after it the other way round - [`crate::schedule`] guarantees
/// both - so the listing tells which. The step after the marks is looked at first: while it is
/// not carried out, the steps before it are as the run left them.
#[must_use]
pub fn applied_count(
    steps: &[RenameStep],
    marked: usize,
    present: &[String],
    fs: &FsRules,
) -> usize {
    let marked = marked.min(steps.len());
    let names: HashSet<String> = present.iter().map(|name| fs.key(name)).collect();
    let holds = |name: &str| names.contains(&fs.key(name));
    if steps
        .get(marked)
        .is_some_and(|step| !holds(&step.from) && holds(&step.to))
    {
        return marked + 1;
    }
    match marked.checked_sub(1).and_then(|last| steps.get(last)) {
        Some(step) if holds(&step.from) && !holds(&step.to) => marked - 1,
        _ => marked,
    }
}

#[cfg(test)]
#[path = "undo_log_tests.rs"]
mod tests;
