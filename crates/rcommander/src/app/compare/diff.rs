//! The diff window's state: what it compares, what it found, and which rows are on screen.
//!
//! Only the rows on screen are drawn ([`ListWindow`]), so a diff of a million lines costs what a
//! page does; outside the edit mode the window has no cursor of its own, its view is all that
//! moves (the edit mode's cursor and its changes: `edit_mode.rs`, RC-100-94). The two sides
//! scroll together because they are one list: each row holds the left and the right line
//! (`TextDiff::rows`).

use std::sync::Arc;

use rc_engine::{
    CancellationToken, VfsPath,
    compare::{EditSession, FileComparison, TextDiff, TextOptions},
};

use super::edit_mode::{Ask, EditCursor, Field, Note, SaveCall, Then};
use super::structure::{DiffView, Structure};
use crate::app::list_window::ListWindow;

/// Rows kept above a difference jumped to, so its context shows.
pub(super) const CONTEXT: usize = 3;

/// What the window shows.
#[derive(Debug, Clone)]
pub enum Shown {
    /// Being read and compared.
    Loading,
    /// Two text files, line by line.
    Text(Arc<TextDiff>),
    /// Files that are binary or too large for the line diff: the bytes compared.
    Bytes(Box<FileComparison>),
    /// The error code of a comparison that failed.
    Failed(&'static str),
}

/// What a comparison of the window's task found.
#[derive(Debug, Clone)]
pub enum Loaded {
    /// Two text files: the diff, and both files as the edit mode changes them.
    Text(Arc<TextDiff>, Box<EditSession>),
    Bytes(Box<FileComparison>),
}

/// An option of the line diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOption {
    Whitespace,
    Case,
    LineEndings,
}

/// The open diff window.
#[derive(Debug)]
pub struct DiffWindow {
    pub left: VfsPath,
    pub right: VfsPath,
    pub options: TextOptions,
    pub shown: Shown,
    /// The blocks of differing lines, counted once when the diff arrives.
    pub changes: usize,
    /// The first row on screen and the rows that fit.
    pub rows: ListWindow,
    /// The start of the difference last jumped to.
    pub current: Option<usize>,
    /// Numbers the comparisons, so a late answer of an earlier one is dropped.
    pub generation: u64,
    /// Stops the running comparison.
    pub cancel: CancellationToken,
    /// Both files as text, as the edit mode changes them; `None` while loading and for the
    /// byte comparison.
    pub session: Option<EditSession>,
    /// The edit mode (F6, Alt+M) and its cursor; `None` outside it.
    pub edit: Option<EditCursor>,
    /// The line being typed in the edit mode.
    pub field: Option<Field>,
    /// The session's generation the diff shown was computed at: an older one is being
    /// compared again.
    pub shown_generation: u64,
    /// A question the window asks over its buttons.
    pub ask: Option<Ask>,
    /// A line under the paths: why a side is read-only, a save that failed.
    pub note: Option<Note>,
    /// The saves running; their answers come back as `EditMsg::Saved`.
    pub saving: Vec<SaveCall>,
    /// What follows once the saves are done: closing, or comparing again.
    pub then: Option<Then>,
    /// The structural comparison of a compare plugin (RC-100-41, `structure.rs`).
    pub structure: Structure,
    /// The text view, or the Structure view while a plugin's comparison is there.
    pub view: DiffView,
}

impl DiffWindow {
    pub fn new(left: VfsPath, right: VfsPath, generation: u64) -> Self {
        Self {
            left,
            right,
            options: TextOptions::default(),
            shown: Shown::Loading,
            changes: 0,
            rows: ListWindow::new(1),
            current: None,
            generation,
            cancel: CancellationToken::new(),
            session: None,
            edit: None,
            field: None,
            shown_generation: 0,
            ask: None,
            note: None,
            saving: Vec::new(),
            then: None,
            structure: Structure::Running,
            view: DiffView::Text,
        }
    }

    /// A new comparison under the current options: the old one stops, its answer is dropped.
    pub fn restart(&mut self, generation: u64) {
        self.cancel.cancel();
        self.cancel = CancellationToken::new();
        self.generation = generation;
        self.shown = Shown::Loading;
        self.changes = 0;
        self.rows.reset();
        self.current = None;
        // Read again from the files: what was changed and not saved is gone.
        self.session = None;
        self.field = None;
        self.ask = None;
        self.note = None;
        self.then = None;
        // Compared again by structure as well; the view the person chose stays.
        self.structure = Structure::Running;
    }

    pub fn set_option(&mut self, option: DiffOption, on: bool) {
        let flag = match option {
            DiffOption::Whitespace => &mut self.options.ignore_whitespace,
            DiffOption::Case => &mut self.options.ignore_case,
            DiffOption::LineEndings => &mut self.options.ignore_line_endings,
        };
        *flag = on;
    }

    /// The comparison's answer; the view starts at the first difference.
    pub fn loaded(&mut self, result: Result<Loaded, &'static str>) {
        self.shown = match result {
            Ok(Loaded::Text(diff, session)) => {
                self.shown_generation = session.generation();
                self.session = Some(*session);
                Shown::Text(diff)
            }
            Ok(Loaded::Bytes(comparison)) => Shown::Bytes(comparison),
            Err(code) => Shown::Failed(code),
        };
        self.changes = self.diff().map_or(0, TextDiff::changes);
        self.rows.reset();
        self.current = None;
        self.next();
        // The edit mode stays on over a new comparison, its cursor on the first difference.
        if self.edit.is_some() {
            self.edit = None;
            self.toggle_edit();
        }
    }

    pub(super) fn diff(&self) -> Option<&TextDiff> {
        match &self.shown {
            Shown::Text(diff) => Some(diff),
            _ => None,
        }
    }

    /// How many rows there are.
    pub fn len(&self) -> usize {
        self.diff().map_or(0, |diff| diff.rows.len())
    }

    /// The indexes of the rows on screen.
    pub fn visible(&self) -> std::ops::Range<usize> {
        self.rows.visible(self.len())
    }

    /// The window was measured: `rows` fit.
    pub fn set_page(&mut self, rows: usize) {
        self.rows.fit(rows, self.len());
    }

    /// The view moves by `rows`, kept within the diff; the next jump starts from the screen.
    pub fn scroll(&mut self, rows: isize) {
        self.rows.scroll(rows, self.len());
        self.current = None;
    }

    pub fn home(&mut self) {
        self.rows.reset();
        self.current = None;
    }

    pub fn end(&mut self) {
        self.rows.set_top(usize::MAX, self.len());
        self.current = None;
    }

    /// Jumps to the next difference: after the one jumped to last, else after the screen's top.
    /// `false` when there is none.
    pub fn next(&mut self) -> bool {
        let Some(diff) = self.diff() else {
            return false;
        };
        let from = self.current.or_else(|| self.rows.top().checked_sub(1));
        let found = diff.next_change(from);
        self.go_to(found)
    }

    /// Jumps to the difference before the one jumped to last, else before the screen's top.
    pub fn previous(&mut self) -> bool {
        let Some(diff) = self.diff() else {
            return false;
        };
        let found = diff.previous_change(self.current.unwrap_or(self.rows.top()));
        self.go_to(found)
    }

    pub(super) fn go_to(&mut self, start: Option<usize>) -> bool {
        let Some(start) = start else {
            return false;
        };
        self.current = Some(start);
        self.rows.set_top(start.saturating_sub(CONTEXT), self.len());
        // In the edit mode the cursor goes along.
        if let Some(cursor) = self.edit.as_mut() {
            cursor.row = start;
        }
        true
    }
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
