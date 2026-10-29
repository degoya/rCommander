//! The synchronize-dirs dialog's state: the form, the plan the comparison found, the rows the
//! filter lets through with a cursor, and the confirmation before the jobs start.
//!
//! Only the rows on screen are drawn ([`ListWindow`]). A person changes a row's action by clicking it or with
//! Space ([`SyncDialog::cycle`]); the plan keeps the change, the filter keeps the row.

use std::sync::Arc;

use rc_engine::{
    CancellationToken, ConflictChoice, DeleteMode, VfsPath,
    sync::{RowFilter, SyncAction, SyncMode, SyncOptions, SyncPlan, SyncRow, SyncRun, SyncSummary},
};

use crate::app::list_window::ListWindow;

/// The actions a click runs through, in order.
const CYCLE: [SyncAction; 5] = [
    SyncAction::None,
    SyncAction::CopyToRight,
    SyncAction::CopyToLeft,
    SyncAction::DeleteRight,
    SyncAction::DeleteLeft,
];

/// The form above the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncForm {
    pub mode: SyncMode,
    pub subdirs: bool,
    pub by_content: bool,
    pub ignore_date: bool,
    /// Seconds two times may lie apart, as typed.
    pub tolerance: String,
    pub ignore_dst_hour: bool,
    /// Total Commander masks, as typed.
    pub masks: String,
}

impl Default for SyncForm {
    fn default() -> Self {
        let options = SyncOptions::default();
        Self {
            mode: options.mode,
            subdirs: options.subdirs,
            by_content: options.by_content,
            ignore_date: options.ignore_date,
            tolerance: options.time_tolerance_secs.to_string(),
            ignore_dst_hour: options.ignore_dst_hour,
            masks: options.masks,
        }
    }
}

impl SyncForm {
    /// The options; `None` while the tolerance is no number of seconds.
    pub fn options(&self) -> Option<SyncOptions> {
        Some(SyncOptions {
            mode: self.mode,
            subdirs: self.subdirs,
            by_content: self.by_content,
            ignore_date: self.ignore_date,
            time_tolerance_secs: self.tolerance.trim().parse().ok()?,
            ignore_dst_hour: self.ignore_dst_hour,
            masks: self.masks.trim().to_owned(),
        })
    }
}

/// A switch of the form or the confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Subdirs,
    ByContent,
    IgnoreDate,
    IgnoreDstHour,
    /// The confirmation: run the deletes.
    Deletes,
    /// The confirmation: delete for good, not into the recycle bin.
    Permanent,
    /// The confirmation: ask before a copy replaces a file.
    AskOverwrite,
}

/// Where the dialog stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Comparing,
    /// A plan is shown.
    Ready,
    /// The plan has no action to run.
    Nothing,
    /// The error code of a comparison or a start that failed.
    Failed(&'static str),
    /// The jobs are being handed to the queue.
    Starting,
}

/// The question before the jobs start: what runs, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub summary: SyncSummary,
    /// The deletes run; only offered when the plan deletes.
    pub deletes: bool,
    pub permanent: bool,
    pub ask_overwrite: bool,
}

impl Confirm {
    pub fn has_deletes(&self) -> bool {
        self.summary.delete_left + self.summary.delete_right > 0
    }
}

/// The open dialog.
#[derive(Debug)]
pub struct SyncDialog {
    pub left: VfsPath,
    pub right: VfsPath,
    pub form: SyncForm,
    pub filter: RowFilter,
    pub phase: Phase,
    pub plan: Option<Arc<SyncPlan>>,
    /// The plan's rows the filter lets through, by index.
    pub shown: Vec<usize>,
    /// The cursor, the first row on screen and the rows that fit, in `shown`.
    pub rows: ListWindow,
    pub confirm: Option<Confirm>,
    /// The tolerance field does not read.
    pub invalid: bool,
    /// Numbers the comparisons, so a late answer of an earlier one is dropped.
    pub generation: u64,
    pub cancel: CancellationToken,
}

impl SyncDialog {
    pub fn new(left: VfsPath, right: VfsPath) -> Self {
        Self {
            left,
            right,
            form: SyncForm::default(),
            filter: RowFilter::Differences,
            phase: Phase::Idle,
            plan: None,
            shown: Vec::new(),
            rows: ListWindow::new(1),
            confirm: None,
            invalid: false,
            generation: 0,
            cancel: CancellationToken::new(),
        }
    }

    /// Starts a comparison numbered `generation`: its options, or `None` (and the field marked)
    /// while the tolerance does not read.
    pub fn start(&mut self, generation: u64) -> Option<SyncOptions> {
        let Some(options) = self.form.options() else {
            self.invalid = true;
            return None;
        };
        self.cancel.cancel();
        self.cancel = CancellationToken::new();
        self.generation = generation;
        self.invalid = false;
        self.phase = Phase::Comparing;
        self.plan = None;
        self.confirm = None;
        self.refilter();
        Some(options)
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }

    /// The comparison's answer.
    pub fn planned(&mut self, result: Result<Arc<SyncPlan>, &'static str>) {
        match result {
            Ok(plan) => {
                self.plan = Some(plan);
                self.phase = Phase::Ready;
            }
            Err(code) => self.phase = Phase::Failed(code),
        }
        self.refilter();
    }

    pub fn set_filter(&mut self, filter: RowFilter) {
        self.filter = filter;
        self.refilter();
    }

    fn refilter(&mut self) {
        let filter = self.filter;
        self.shown = self.plan.as_ref().map_or_else(Vec::new, |plan| {
            (0..plan.rows.len())
                .filter(|&index| filter.passes(&plan.rows[index]))
                .collect()
        });
        self.rows.reset();
    }

    /// The row at `shown` place `at`, with its index in the plan.
    pub fn row(&self, at: usize) -> Option<(usize, &SyncRow)> {
        let index = *self.shown.get(at)?;
        Some((index, &self.plan.as_ref()?.rows[index]))
    }

    pub fn summary(&self) -> Option<SyncSummary> {
        self.plan.as_ref().map(|plan| plan.summary())
    }

    /// The indexes into `shown` of the rows on screen.
    pub fn visible(&self) -> std::ops::Range<usize> {
        self.rows.visible(self.shown.len())
    }

    /// The list is `rows` high now; the view stays as full as it can, the cursor on it.
    pub fn set_page(&mut self, rows: usize) {
        self.rows.set_page(rows, self.shown.len());
    }

    /// The cursor moves by `by` rows (clamped) and stays on screen.
    pub fn step(&mut self, by: isize) {
        self.rows.step(by, self.shown.len());
    }

    pub fn point_at(&mut self, at: usize) {
        self.rows.point_at(at, self.shown.len());
    }

    /// The wheel: the view moves, the cursor stays.
    pub fn scroll(&mut self, rows: isize) {
        self.rows.scroll(rows, self.shown.len());
    }

    /// The row at `at` takes the next action it allows, after its own.
    pub fn cycle(&mut self, at: usize) {
        let Some((index, row)) = self.row(at) else {
            return;
        };
        let from = CYCLE
            .iter()
            .position(|action| *action == row.action)
            .unwrap_or(0);
        let next = (1..=CYCLE.len())
            .map(|step| CYCLE[(from + step) % CYCLE.len()])
            .find(|action| row.allows(*action));
        if let (Some(action), Some(plan)) = (next, self.plan.as_mut()) {
            // `allows` was asked: the action fits the row.
            let _ = Arc::make_mut(plan).set_action(index, action);
        }
    }

    /// Opens the confirmation; `false` (and the phase says so) when no row has an action.
    pub fn ask(&mut self) -> bool {
        let Some(summary) = self.summary() else {
            return false;
        };
        let anything = self
            .plan
            .as_ref()
            .is_some_and(|plan| plan.rows.iter().any(|row| row.action != SyncAction::None));
        if !anything {
            self.phase = Phase::Nothing;
            return false;
        }
        self.confirm = Some(Confirm {
            summary,
            deletes: true,
            permanent: false,
            ask_overwrite: true,
        });
        true
    }

    /// What the confirmation runs: the plan - without its deletes when they were unticked - and
    /// how its jobs run. `None` without a confirmation.
    pub fn confirmed(&self) -> Option<(SyncPlan, SyncRun)> {
        let confirm = self.confirm.as_ref()?;
        let mut plan = SyncPlan::clone(self.plan.as_ref()?);
        let deletes = confirm.deletes && confirm.has_deletes();
        if !deletes {
            for row in &mut plan.rows {
                if matches!(row.action, SyncAction::DeleteLeft | SyncAction::DeleteRight) {
                    row.action = SyncAction::None;
                }
            }
        }
        let run = SyncRun {
            deletes_confirmed: deletes,
            delete_mode: if confirm.permanent {
                DeleteMode::Permanent
            } else {
                DeleteMode::Trash
            },
            on_conflict: (!confirm.ask_overwrite).then_some(ConflictChoice::Overwrite),
            ..SyncRun::default()
        };
        Some((plan, run))
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
