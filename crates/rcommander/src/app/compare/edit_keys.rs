//! The edit mode's keys and messages (RC-100-94): what the window does with them, the diff
//! computed again after each change, the saves and the questions before closing or comparing
//! again with changes not saved. The state itself is `edit_mode.rs`.

use std::sync::Arc;

use iced::{
    Task,
    widget::{Id, operation},
};
use rc_engine::{
    Chord, Engine,
    compare::{CompareSide, Direction, Origin, TextDiff, TextFile, TextOptions},
};

use super::{CompareMsg, DiffWindow, SaveCall, Then, edit_mode::Ask, wrap};
use crate::app::{App, Effect};

/// The id of the field a line is typed in.
pub const FIELD_ID: &str = "compare.edit_field";

/// A message of the edit mode.
#[derive(Debug, Clone)]
pub enum EditMsg {
    /// F6, Alt+M and the button "Edit mode".
    Toggle,
    /// "Copy ->", "Copy <-", Alt+Right, Alt+Left.
    Copy(Direction),
    /// "Undo", Ctrl+Z, Alt+Backspace (`true`); Ctrl+Y (`false`).
    Undo(bool),
    /// "Save" and Ctrl+S.
    Save,
    /// The field's text as typed.
    Field(String),
    /// Enter in the field.
    Commit,
    /// A click on a line: its row and side.
    Click(usize, CompareSide),
    /// The diff of the window's comparison `generation` computed at the session's generation;
    /// `None` when the runtime gave up on it.
    Computed(u64, u64, Option<Arc<TextDiff>>),
    /// A save of the window's comparison `generation` answered.
    Saved(u64, CompareSide, Result<Origin, &'static str>),
    /// "Compare": both files read and compared again.
    Reload,
    /// An answer to the window's question.
    Answer(Answer),
}

/// The answers of the window's questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Overwrite,
    Reload,
    Cancel,
}

/// The diff of `files` under `options`, off the window's thread; `None` when the runtime gave
/// up on it.
pub(in crate::app) async fn compute(
    files: (TextFile, TextFile),
    options: TextOptions,
) -> Option<Arc<TextDiff>> {
    rc_engine::compare::diff_texts(files.0, files.1, &options)
        .await
        .ok()
        .map(Arc::new)
}

/// One save as its task runs it.
pub(in crate::app) async fn save(engine: Engine, call: SaveCall) -> Result<Origin, &'static str> {
    engine
        .save_compared(&call.origin, &call.bytes, call.force)
        .await
        .map_err(|error| error.code())
}

fn edit(message: EditMsg) -> CompareMsg {
    CompareMsg::Edit(message)
}

impl App {
    fn diff_window(&mut self) -> Option<&mut DiffWindow> {
        self.compare.diff.as_mut()
    }

    /// A key of the edit mode, of its field or of its question; `None` for a key the window's
    /// view keys take (`App::diff_key`).
    pub(super) fn diff_edit_key(&mut self, chord: &Chord, captured: bool) -> Option<Vec<Effect>> {
        let window = self.compare.diff.as_ref()?;
        let plain = !chord.ctrl && !chord.alt && !chord.shift;
        let key = chord.key.as_str();
        if let Some(ask) = window.ask {
            // The question has the keys: Enter takes its first answer, Escape cancels.
            let answer = match (plain, key) {
                (true, "Escape") => Some(Answer::Cancel),
                (true, "Enter") if !captured => Some(match ask {
                    Ask::Unsaved(_) => Answer::Save,
                    Ask::Changed(_) => Answer::Overwrite,
                }),
                _ => None,
            };
            return Some(
                answer.map_or_else(Vec::new, |answer| self.on_edit(EditMsg::Answer(answer))),
            );
        }
        if window.field.is_some() {
            // The field has the keys; Enter is its submit, which a captured Enter already sent.
            return Some(match (plain, key) {
                (true, "Escape") => {
                    self.with_edit(DiffWindow::cancel_field);
                    Vec::new()
                }
                (true, "Enter") if !captured => self.on_edit(EditMsg::Commit),
                _ => Vec::new(),
            });
        }
        let editing = window.edit.is_some();
        let message = match (chord.ctrl, chord.alt, chord.shift, key) {
            (false, false, false, "F6") => EditMsg::Toggle,
            // A pressed access key arrives captured: the button "Edit mode" took it.
            (false, true, false, "m") if !captured => EditMsg::Toggle,
            (true, false, false, "s") => EditMsg::Save,
            (true, false, false, "z") | (false, true, false, "Backspace") => EditMsg::Undo(true),
            (true, false, false, "y") => EditMsg::Undo(false),
            (false, true, false, "ArrowRight") if editing => EditMsg::Copy(Direction::LeftToRight),
            (false, true, false, "ArrowLeft") if editing => EditMsg::Copy(Direction::RightToLeft),
            _ if editing => return self.cursor_key(chord),
            _ => return None,
        };
        Some(self.on_edit(message))
    }

    /// The edit mode's cursor keys; `None` for the view keys (Alt+Down, Alt+Up, Escape).
    fn cursor_key(&mut self, chord: &Chord) -> Option<Vec<Effect>> {
        let window = self.diff_window()?;
        let page = window.rows.page() as isize;
        match (chord.ctrl, chord.alt, chord.shift, chord.key.as_str()) {
            (false, false, false, "ArrowDown") => window.move_cursor(1),
            (false, false, false, "ArrowUp") => window.move_cursor(-1),
            (false, false, false, "PageDown") => window.move_cursor(page),
            (false, false, false, "PageUp") => window.move_cursor(-page),
            (false, false, false, "Home") => window.move_cursor(isize::MIN),
            (false, false, false, "End") => window.move_cursor(isize::MAX),
            (true, false, false, "ArrowLeft") => window.set_side(CompareSide::Left),
            (true, false, false, "ArrowRight") => window.set_side(CompareSide::Right),
            (false, false, false, "Enter" | "Insert") => {
                if window.start_field(chord.key == "Insert") {
                    return Some(vec![Effect::Task(operation::focus(Id::new(FIELD_ID)))]);
                }
            }
            (false, false, false, "Delete") => {
                if window.delete_line() {
                    return Some(self.recompute());
                }
            }
            _ => return None,
        }
        Some(Vec::new())
    }

    fn with_edit(&mut self, change: impl FnOnce(&mut DiffWindow) -> bool) -> bool {
        self.diff_window().is_some_and(change)
    }

    /// The edit mode's messages.
    pub(super) fn on_edit(&mut self, message: EditMsg) -> Vec<Effect> {
        match message {
            EditMsg::Toggle => {
                self.with_edit(DiffWindow::toggle_edit);
            }
            EditMsg::Copy(direction) => {
                if self.with_edit(|window| window.copy(direction)) {
                    return self.recompute();
                }
            }
            EditMsg::Undo(back) => {
                if self.with_edit(|window| window.undo_redo(back)) {
                    return self.recompute();
                }
            }
            EditMsg::Save => return self.saves(None, false),
            EditMsg::Field(text) => {
                if let Some(window) = self.diff_window() {
                    window.type_field(text);
                }
            }
            EditMsg::Commit => {
                if self.with_edit(DiffWindow::commit_field) {
                    return self.recompute();
                }
            }
            EditMsg::Click(row, side) => {
                if let Some(window) = self.diff_window() {
                    window.click(row, side);
                }
            }
            EditMsg::Computed(generation, at, Some(diff)) => {
                if let Some(window) = self
                    .diff_window()
                    .filter(|window| window.generation == generation)
                {
                    window.computed(at, diff);
                }
            }
            EditMsg::Computed(_, _, None) => {}
            EditMsg::Saved(generation, side, result) => {
                let then = self
                    .diff_window()
                    .filter(|window| window.generation == generation)
                    .and_then(|window| window.saved(side, result));
                if let Some(then) = then {
                    return self.carry_out(then);
                }
            }
            EditMsg::Reload => return self.unless_unsaved(Then::Reload),
            EditMsg::Answer(answer) => return self.compare_answer(answer),
        }
        Vec::new()
    }

    /// Closing or comparing again: at once, or after the question when a side is not saved.
    pub(super) fn unless_unsaved(&mut self, then: Then) -> Vec<Effect> {
        match self.diff_window() {
            Some(window) if window.dirty() => {
                window.ask = Some(Ask::Unsaved(then));
                Vec::new()
            }
            Some(_) => self.carry_out(then),
            None => Vec::new(),
        }
    }

    fn carry_out(&mut self, then: Then) -> Vec<Effect> {
        match then {
            Then::Close => {
                self.close_diff();
                Vec::new()
            }
            Then::Reload => self.reload_diff(),
        }
    }

    fn compare_answer(&mut self, answer: Answer) -> Vec<Effect> {
        let Some(window) = self.diff_window() else {
            return Vec::new();
        };
        let Some(ask) = window.ask.take() else {
            return Vec::new();
        };
        match (ask, answer) {
            (Ask::Unsaved(then), Answer::Save) => {
                window.then = Some(then);
                self.saves(None, false)
            }
            (Ask::Unsaved(then), Answer::Discard) => self.carry_out(then),
            (Ask::Changed(side), Answer::Overwrite) => self.saves(Some(side), true),
            (Ask::Changed(_), Answer::Reload) => self.reload_diff(),
            // Cancel, and an answer the question does not offer.
            _ => {
                window.then = None;
                Vec::new()
            }
        }
    }

    /// The saves of the changed sides as tasks; when nothing is left to save, what was to
    /// follow follows.
    fn saves(&mut self, only: Option<CompareSide>, force: bool) -> Vec<Effect> {
        let Some(window) = self.diff_window() else {
            return Vec::new();
        };
        let calls = window.start_saves(only, force);
        let generation = window.generation;
        if calls.is_empty() && window.saving.is_empty() && !window.dirty() {
            if let Some(then) = window.then.take() {
                return self.carry_out(then);
            }
            return Vec::new();
        }
        calls
            .into_iter()
            .map(|call| {
                let side = call.side;
                Effect::Task(Task::perform(
                    save(self.engine.clone(), call),
                    move |result| wrap(edit(EditMsg::Saved(generation, side, result))),
                ))
            })
            .collect()
    }

    /// The diff of the text as it is now, computed off the window's thread.
    pub(super) fn recompute(&self) -> Vec<Effect> {
        let Some(window) = &self.compare.diff else {
            return Vec::new();
        };
        let Some(session) = &window.session else {
            return Vec::new();
        };
        let (generation, at) = (window.generation, session.generation());
        let task = Task::perform(compute(session.files(), window.options), move |diff| {
            wrap(edit(EditMsg::Computed(generation, at, diff)))
        });
        vec![Effect::Task(task)]
    }
}

#[cfg(test)]
#[path = "edit_keys_tests.rs"]
mod tests;
