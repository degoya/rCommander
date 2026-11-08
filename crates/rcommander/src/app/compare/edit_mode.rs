//! The diff window's edit mode (RC-100-94), Total Commander's "Edit mode" of Compare by content,
//! by lines (owner, 2026-11-07): F6 or Alt+M switch it, a cursor marks a row and a side, Enter
//! edits that side's line in a field, Insert adds a line, Delete removes one, Alt+Right and
//! Alt+Left (the buttons "Copy ->" and "Copy <-") copy the cursor's block of differences over,
//! Ctrl+Z and Alt+Backspace undo, Ctrl+Y redoes, Ctrl+S saves.
//!
//! The text lives in the engine's `EditSession`; after every change the window computes the diff
//! again off its thread, and until that answer is in no further change is made - every change
//! works along a diff of the text as it is (`DiffWindow::fresh`). While a save runs nothing
//! changes either, so a side is marked saved with the text that was written.

use std::sync::Arc;

use rc_engine::compare::{
    CompareSide, Direction, EditSession, Origin, ReadOnly, TextDiff, Unmappable, edit,
};

use super::{DiffWindow, Shown};

/// The edit mode's cursor: a row of the diff and a side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditCursor {
    pub row: usize,
    pub side: CompareSide,
}

/// The line being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    /// A new line, before the cursor's line on its side; else the cursor's line changed.
    pub insert: bool,
}

/// What follows once the changes are saved or discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Close,
    /// Compare again, reading both files anew.
    Reload,
}

/// A question of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Closing or comparing again with changes not saved: Save, Discard, Cancel.
    Unsaved(Then),
    /// A side's file changed since it was read: Overwrite, Reload, Cancel.
    Changed(CompareSide),
}

/// A line the window shows under the paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    ReadOnly(CompareSide, ReadOnly),
    /// A side holds a character its encoding cannot write; nothing was saved.
    Unmappable(CompareSide, Unmappable),
    /// A save failed with this error code.
    Failed(&'static str),
}

/// A save of one side, as its task runs it.
#[derive(Debug, Clone)]
pub struct SaveCall {
    pub side: CompareSide,
    pub origin: Origin,
    pub bytes: Arc<Vec<u8>>,
    /// Over a file that changed since it was read.
    pub force: bool,
}

impl DiffWindow {
    /// The diff shown, when it is of the text as it is now and no save or question holds the
    /// window: what a change works along.
    pub fn fresh(&self) -> Option<Arc<TextDiff>> {
        if !self.saving.is_empty() || self.ask.is_some() {
            return None;
        }
        match (&self.shown, &self.session) {
            (Shown::Text(diff), Some(session)) if session.generation() == self.shown_generation => {
                Some(Arc::clone(diff))
            }
            _ => None,
        }
    }

    /// A side was changed and not saved.
    pub fn dirty(&self) -> bool {
        self.session.as_ref().is_some_and(EditSession::any_dirty)
    }

    /// F6 and Alt+M: the edit mode on - the cursor on the difference shown, on the left unless
    /// only the right can change, a read-only side's reason under the paths - or off. `false`
    /// when there is no text to edit.
    pub fn toggle_edit(&mut self) -> bool {
        if self.edit.take().is_some() {
            self.field = None;
            return true;
        }
        let Some(session) = &self.session else {
            return false;
        };
        let side = if !session.editable(CompareSide::Left) && session.editable(CompareSide::Right) {
            CompareSide::Right
        } else {
            CompareSide::Left
        };
        self.note = [CompareSide::Left, CompareSide::Right]
            .into_iter()
            .find_map(|side| session.read_only(side).map(|why| Note::ReadOnly(side, why)));
        let row = self
            .current
            .unwrap_or(self.rows.top())
            .min(self.len().saturating_sub(1));
        self.edit = Some(EditCursor { row, side });
        self.current = Some(row);
        true
    }

    /// The cursor `by` rows on, kept within the diff and on screen.
    pub fn move_cursor(&mut self, by: isize) {
        let len = self.len();
        let Some(cursor) = self.edit.as_mut() else {
            return;
        };
        if len == 0 || self.field.is_some() {
            return;
        }
        cursor.row = cursor.row.saturating_add_signed(by).min(len - 1);
        let row = cursor.row;
        self.current = Some(row);
        self.reveal(row);
    }

    /// Scrolls `row` into view.
    fn reveal(&mut self, row: usize) {
        let (top, page, len) = (self.rows.top(), self.rows.page().max(1), self.len());
        if row < top {
            self.rows.set_top(row, len);
        } else if row >= top + page {
            self.rows.set_top(row + 1 - page, len);
        }
    }

    /// Ctrl+Left, Ctrl+Right: the cursor to a side.
    pub fn set_side(&mut self, side: CompareSide) {
        if self.field.is_none()
            && let Some(cursor) = self.edit.as_mut()
        {
            cursor.side = side;
        }
    }

    /// A click on a line in the edit mode: the cursor there.
    pub fn click(&mut self, row: usize, side: CompareSide) {
        if self.field.is_some() || row >= self.len() {
            return;
        }
        if let Some(cursor) = self.edit.as_mut() {
            *cursor = EditCursor { row, side };
            self.current = Some(row);
        }
    }

    /// The cursor's line of its side, as the diff shown has it.
    fn cursor_line(&self, diff: &TextDiff) -> Option<usize> {
        let cursor = self.edit?;
        diff.rows
            .get(cursor.row)
            .and_then(|row| cursor.side.line_of(row))
    }

    /// Enter (`insert` false: the cursor's line into the field) or Insert (a new line). A row
    /// without a line on the cursor's side always gets a new one.
    pub fn start_field(&mut self, insert: bool) -> bool {
        let (Some(cursor), Some(diff), Some(session)) = (self.edit, self.fresh(), &self.session)
        else {
            return false;
        };
        if self.field.is_some() || !session.editable(cursor.side) {
            return false;
        }
        let line = self.cursor_line(&diff);
        let text = match (insert, line) {
            (false, Some(index)) => session
                .file(cursor.side)
                .lines
                .get(index)
                .map(|line| line.text.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.field = Some(Field {
            text,
            insert: insert || line.is_none(),
        });
        true
    }

    /// The field's text as typed.
    pub fn type_field(&mut self, text: String) {
        if let Some(field) = self.field.as_mut() {
            field.text = text;
        }
    }

    /// Escape in the field: nothing changes.
    pub fn cancel_field(&mut self) -> bool {
        self.field.take().is_some()
    }

    /// Enter in the field: the line changed, or the new line in before the cursor's.
    pub fn commit_field(&mut self) -> bool {
        let (Some(cursor), Some(diff)) = (self.edit, self.fresh()) else {
            return false;
        };
        let Some(field) = self.field.take() else {
            return false;
        };
        let line = self.cursor_line(&diff);
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        match (field.insert, line) {
            (false, Some(index)) => session.set_line(cursor.side, index, field.text),
            (true, Some(index)) => session.insert_line(cursor.side, index, field.text),
            (_, None) => {
                // A row without a line on this side: before the next line it has.
                let at = diff.position(cursor.row);
                let at = match cursor.side {
                    CompareSide::Left => at.left,
                    CompareSide::Right => at.right,
                };
                session.insert_line(cursor.side, at - 1, field.text)
            }
        }
    }

    /// Delete: the cursor's line removed from its side.
    pub fn delete_line(&mut self) -> bool {
        let (Some(cursor), Some(diff)) = (self.edit, self.fresh()) else {
            return false;
        };
        if self.field.is_some() {
            return false;
        }
        let Some(index) = self.cursor_line(&diff) else {
            return false;
        };
        self.session
            .as_mut()
            .is_some_and(|session| session.delete_line(cursor.side, index))
    }

    /// "Copy ->" and "Copy <-": the cursor's block of differences - or its own line, on an
    /// equal row - copied over.
    pub fn copy(&mut self, direction: Direction) -> bool {
        let (Some(cursor), Some(diff)) = (self.edit, self.fresh()) else {
            return false;
        };
        if self.field.is_some() {
            return false;
        }
        let rows = edit::rows_at(&diff, cursor.row);
        self.session
            .as_mut()
            .is_some_and(|session| session.copy_rows(&diff, rows, direction))
    }

    /// Ctrl+Z, Alt+Backspace (`back`) and Ctrl+Y.
    pub fn undo_redo(&mut self, back: bool) -> bool {
        if self.edit.is_none() || !self.saving.is_empty() || self.ask.is_some() {
            return false;
        }
        self.field = None;
        self.session.as_mut().is_some_and(
            |session| {
                if back { session.undo() } else { session.redo() }
            },
        )
    }

    /// The diff computed after a change at `generation`: shown when it is of the text as it is
    /// now; the cursor stays where it was, within the rows.
    pub fn computed(&mut self, generation: u64, diff: Arc<TextDiff>) -> bool {
        if self.session.as_ref().map(EditSession::generation) != Some(generation) {
            return false;
        }
        self.shown = Shown::Text(diff);
        self.shown_generation = generation;
        self.changes = self.diff().map_or(0, TextDiff::changes);
        let len = self.len();
        self.rows.fit(self.rows.page(), len);
        if let Some(cursor) = self.edit.as_mut() {
            cursor.row = cursor.row.min(len.saturating_sub(1));
            self.current = Some(cursor.row);
        }
        true
    }

    /// The saves of every side changed and not yet being saved - or of `only` - recorded as
    /// running. A side that cannot be written in its encoding saves nothing and says why.
    pub fn start_saves(&mut self, only: Option<CompareSide>, force: bool) -> Vec<SaveCall> {
        let Some(session) = &self.session else {
            return Vec::new();
        };
        let mut calls = Vec::new();
        for side in [CompareSide::Left, CompareSide::Right] {
            let skip = only.is_some_and(|only| only != side)
                || !session.dirty(side)
                || !session.editable(side)
                || self.saving.iter().any(|call| call.side == side);
            if skip {
                continue;
            }
            match session.encoded(side) {
                Ok(bytes) => calls.push(SaveCall {
                    side,
                    origin: session.origin(side).clone(),
                    bytes: Arc::new(bytes),
                    force,
                }),
                Err(unmappable) => {
                    self.note = Some(Note::Unmappable(side, unmappable));
                    self.then = None;
                    return Vec::new();
                }
            }
        }
        self.saving.extend(calls.iter().cloned());
        calls
    }

    /// A save's answer: the side is clean, or the window asks (`compare_changed`) or says why
    /// it failed. Answers what follows when it was the last save and nothing is left changed.
    pub fn saved(
        &mut self,
        side: CompareSide,
        result: Result<Origin, &'static str>,
    ) -> Option<Then> {
        self.saving.retain(|call| call.side != side);
        match result {
            Ok(origin) => {
                if let Some(session) = self.session.as_mut() {
                    session.saved(side, origin);
                }
                self.note = None;
                if self.saving.is_empty() && !self.dirty() {
                    return self.then.take();
                }
            }
            Err("compare_changed") => self.ask = Some(Ask::Changed(side)),
            Err(code) => {
                self.note = Some(Note::Failed(code));
                self.then = None;
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "edit_mode_tests.rs"]
mod tests;
