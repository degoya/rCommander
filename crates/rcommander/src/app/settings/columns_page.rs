//! The Columns tab of the Appearance page (RC-100-90): the column sets on the left - New, Copy,
//! Up, Down and Delete - and the marked set's editor on the right: its name, a table of its
//! columns (field, title, width, alignment, each row moved up or down or removed), its folder
//! masks and Save. The rows `columns.auto_switch` and `columns.auto_default` stand below.
//!
//! The sets are the window's (`app/resize/sets.rs`, read again after every change); each button
//! sends its `column_sets.*` command off the window's thread and the answer comes back as
//! [`ColumnsMsg::Answered`]. The editor keeps its edits until Save, while the same set stays
//! marked; Save renames the set when its name changed, then stores its columns and masks. The
//! engine checks everything again - the limits, the name - and its refusal is the tab's note.

mod actions;
mod view;

use rc_engine::{Align, Column, ColumnSet, ColumnSetSpec, Field, columns::SPEC_VERSION};

pub(super) use self::view::view;
use super::{SettingsMessage, wrap};
use crate::app::{App, Effect, Message};

/// The masks of a set as the editor's one field writes them: separated by `;`.
const MASK_SEPARATOR: &str = ";";

/// A column of the editor: its field and alignment, its title and width as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub field: Field,
    pub title: String,
    pub width: String,
    pub align: Align,
}

/// The marked set as edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    pub id: i64,
    pub name: String,
    pub rows: Vec<Row>,
    pub masks: String,
    /// Something was edited since the set was read.
    pub dirty: bool,
}

impl Editor {
    /// The editor of `set` as stored.
    fn of(set: &ColumnSet) -> Self {
        Self {
            id: set.id,
            name: set.name.clone(),
            rows: set
                .spec
                .columns
                .iter()
                .map(|column| Row {
                    field: column.field,
                    title: column.title.clone().unwrap_or_default(),
                    width: column.width.to_string(),
                    align: column.align,
                })
                .collect(),
            masks: set.spec.masks.join(MASK_SEPARATOR),
            dirty: false,
        }
    }

    /// The spec the editor holds; a width that is no number is the code `column_set_invalid`.
    fn spec(&self) -> Result<ColumnSetSpec, &'static str> {
        let columns = self
            .rows
            .iter()
            .map(|row| {
                let width = row.width.trim().parse().map_err(|_| "column_set_invalid")?;
                let title = row.title.trim();
                Ok(Column {
                    field: row.field,
                    title: (!title.is_empty()).then(|| title.to_owned()),
                    width,
                    align: row.align,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        let masks = self
            .masks
            .split(MASK_SEPARATOR)
            .map(str::trim)
            .filter(|mask| !mask.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(ColumnSetSpec {
            version: SPEC_VERSION,
            columns,
            masks,
        })
    }
}

/// The tab's state while the dialog is open.
#[derive(Debug, Default)]
pub struct ColumnsPage {
    /// The sets as last read, in the person's order.
    sets: Vec<ColumnSet>,
    /// The id of the set marked in the list.
    marked: Option<i64>,
    editor: Option<Editor>,
    /// The last outcome: a text, and whether it is an error.
    note: Option<(String, bool)>,
    /// A command runs; the buttons wait.
    busy: bool,
}

impl ColumnsPage {
    /// The sets read again: the marked one stays marked while it is there (else the first), its
    /// editor keeps unsaved edits, and is read again otherwise.
    pub(in crate::app) fn sets_read(&mut self, sets: &[ColumnSet]) {
        self.sets = sets.to_vec();
        if !self.marked.is_some_and(|id| self.index_of(id).is_some()) {
            self.marked = self.sets.first().map(|set| set.id);
        }
        let keep = self
            .editor
            .as_ref()
            .is_some_and(|editor| editor.dirty && Some(editor.id) == self.marked);
        if !keep {
            self.editor = self.marked_set().map(Editor::of);
        }
    }

    fn index_of(&self, id: i64) -> Option<usize> {
        self.sets.iter().position(|set| set.id == id)
    }

    fn marked_set(&self) -> Option<&ColumnSet> {
        self.sets.get(self.index_of(self.marked?)?)
    }
}

/// The tab's messages.
#[derive(Debug, Clone)]
pub enum ColumnsMsg {
    /// A set marked in the list.
    Choose(i64),
    New,
    Copy,
    Up,
    Down,
    Delete,
    Name(String),
    Masks(String),
    Field(usize, Field),
    Title(usize, String),
    Width(usize, String),
    Align(usize, Align),
    RowUp(usize),
    RowDown(usize),
    RowRemove(usize),
    AddRow,
    Save,
    /// A command's answer: the set it saved (to mark), or the code of why not.
    Answered(Result<Option<i64>, &'static str>),
}

/// The message of the tab.
pub(super) fn page_message(message: ColumnsMsg) -> Message {
    wrap(SettingsMessage::Columns(message))
}

impl App {
    /// A message of the Columns tab.
    pub(super) fn columns_message(&mut self, message: ColumnsMsg) -> Vec<Effect> {
        let engine = self.engine.clone();
        let Some(page) = self.settings.as_mut().map(|dialog| &mut dialog.columns) else {
            return Vec::new();
        };
        actions::apply(page, &engine, message)
    }

    /// The sets the window read, into the Columns tab while the dialog is open.
    pub(in crate::app) fn columns_page_read(&mut self, sets: &[ColumnSet]) {
        if let Some(dialog) = self.settings.as_mut() {
            dialog.columns.sets_read(sets);
        }
    }
}

#[cfg(test)]
#[path = "columns_page_tests.rs"]
mod tests;
