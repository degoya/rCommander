//! The queue window's selection (RC-010-15, owner 2026-10-18), as in Windows and Total
//! Commander: a click selects one job, Ctrl+click marks or unmarks one, Shift+click marks the
//! jobs from the anchor to it; Space and Insert mark or unmark the cursor's job and move on,
//! Shift+arrows mark the jobs from the anchor as the cursor moves, Ctrl+A marks all. The plain
//! arrows move the cursor and leave the marks, as in Total Commander's panels.
//!
//! The window's actions take [`QueueWindow::chosen`]: the marked jobs, or the cursor's job while
//! none is marked.

use std::collections::BTreeSet;

use rc_engine::{JobId, QueueEntry};

use super::QueueWindow;

impl QueueWindow {
    /// The jobs the actions take, in the window's order: the marked ones, else the cursor's.
    pub fn chosen(&self) -> Vec<JobId> {
        self.chosen_entries()
            .into_iter()
            .map(|entry| entry.job.id)
            .collect()
    }

    /// The entries of [`QueueWindow::chosen`].
    pub fn chosen_entries(&self) -> Vec<&QueueEntry> {
        if self.marked.is_empty() {
            return self.selected_entry().into_iter().collect();
        }
        self.entries
            .iter()
            .filter(|entry| self.marked.contains(&entry.job.id))
            .collect()
    }

    /// A click on the row of `job`: with `shift` the jobs from the anchor to it are marked, with
    /// `ctrl` it is marked or unmarked - the cursor's job is marked first while none is, so the
    /// click adds to it -, else it alone is selected. The cursor goes to it.
    pub fn click(&mut self, job: JobId, ctrl: bool, shift: bool) {
        if shift {
            let anchor = self.anchor.or(self.selected).unwrap_or(job);
            self.marked = self.range(anchor, job);
            self.anchor = Some(anchor);
        } else {
            if !ctrl {
                self.marked.clear();
            } else if self.marked.is_empty() {
                self.marked
                    .extend(self.selected.filter(|cursor| *cursor != job));
                self.marked.insert(job);
            } else if !self.marked.remove(&job) {
                self.marked.insert(job);
            }
            self.anchor = Some(job);
        }
        self.selected = Some(job);
    }

    /// Shift+Up (`-1`) or Shift+Down (`1`): the cursor moves, the jobs from the anchor to it are
    /// marked.
    pub fn extend(&mut self, by: isize) {
        let anchor = self.anchor.or(self.selected);
        self.step(by);
        if let (Some(anchor), Some(cursor)) = (anchor.or(self.selected), self.selected) {
            self.marked = self.range(anchor, cursor);
            self.anchor = Some(anchor);
        }
    }

    /// Space and Insert: the cursor's job marked or unmarked, the cursor one row down.
    pub fn toggle_and_step(&mut self) {
        let Some(job) = self.selected else {
            return;
        };
        if !self.marked.remove(&job) {
            self.marked.insert(job);
        }
        self.step(1);
        self.anchor = self.selected;
    }

    /// Ctrl+A: every job of the list marked.
    pub fn select_all(&mut self) {
        self.marked = self.entries.iter().map(|entry| entry.job.id).collect();
    }

    /// No job marked, no anchor: another list is shown.
    pub(super) fn unmark(&mut self) {
        self.marked.clear();
        self.anchor = None;
    }

    /// The marks and the anchor kept on the jobs still listed.
    pub(super) fn keep_listed(&mut self) {
        let listed: BTreeSet<JobId> = self.entries.iter().map(|entry| entry.job.id).collect();
        self.marked.retain(|job| listed.contains(job));
        self.anchor = self.anchor.filter(|job| listed.contains(job));
    }

    /// The jobs from `from` to `to` in the window's order, both included, either way round; `to`
    /// alone when one of them is not listed.
    fn range(&self, from: JobId, to: JobId) -> BTreeSet<JobId> {
        let at = |job: JobId| self.entries.iter().position(|entry| entry.job.id == job);
        let (Some(from), Some(upto)) = (at(from), at(to)) else {
            return BTreeSet::from([to]);
        };
        self.entries[from.min(upto)..=from.max(upto)]
            .iter()
            .map(|entry| entry.job.id)
            .collect()
    }
}

/// Whether taking `entry` off cancels work, so the window asks first: it has not finished and
/// runs, is paused or asks a question, or has done part of its work already (an interrupted
/// transfer, a job paused after it had begun).
pub fn started(entry: &QueueEntry) -> bool {
    let job = &entry.job;
    !job.state.is_finished()
        && (entry.place.is_none()
            || job.resumable
            || job.progress.bytes_done > 0
            || job.progress.files_done > 0)
}

#[cfg(test)]
#[path = "selection_tests.rs"]
pub(super) mod tests;
