//! One panel as the window draws it: the engine's state, copied on every change, and what only
//! the screen knows - the `..` row, the first row shown, the rows and columns that fit, the
//! status line's sums and the last error.
//!
//! The `..` row is the window's, as in Total Commander: it heads every listing that has a parent
//! and is no entry of the engine, so MCP sees the directory as it is. The cursor can stand on it
//! before the first entry ([`Pane::through_parent_row`]); Enter there goes up, it is never
//! selected, and Insert or Space there step onto the first entry. While the cursor stands on it
//! the engine's cursor is on the first entry.
//!
//! The full view shows one row per entry; the brief view fills columns top to bottom, as many as
//! the panel is wide, and scrolls by whole columns. Either way only the rows in
//! [`Pane::visible_range`] are ever built into widgets; that is the whole virtualisation, and it
//! keeps a 100 000-entry folder as cheap to draw as a 30-entry one.
//!
//! Pure data and arithmetic, no iced types, so all of it is tested without a window.

mod summary;

use std::{ops::Range, sync::Arc};

use rc_engine::{
    Command, CursorStep, CursorTarget, Entry, ListMode, PanelId, PanelState, extension,
};

pub use summary::Summary;

/// The width of one column of the brief view, in logical pixels.
pub const BRIEF_COLUMN_WIDTH: f32 = 200.0;

#[derive(Debug)]
pub struct Pane {
    /// The engine's state of this panel; the window never changes it, only replaces it.
    pub state: PanelState,
    top: usize,
    page_rows: usize,
    /// The columns of the brief view that fit.
    columns: usize,
    /// The list's measured width; 0 until the first measure.
    width: f32,
    /// Whether the cursor stands on the `..` row (when there is one).
    on_parent: bool,
    /// The window put its cursor on `..` itself (Home, a click on `..`, ...) and the engine's
    /// answer, its cursor on the first entry, is still to come: that answer keeps `..`.
    parent_asked: bool,
    /// The status line's counts and sums, worked out when entries or selection change.
    summary: Summary,
    /// The last command that failed on this panel, shown in its status line until the panel
    /// shows another directory.
    pub error: Option<String>,
    /// The follow notice the window put into `error` itself, which it clears by itself when the
    /// terminal can follow again - never another message (RC-080-53).
    pub follow_notice: Option<&'static str>,
}

impl Pane {
    /// A freshly listed directory: the cursor on `..`, as after entering it.
    pub fn new(state: PanelState) -> Self {
        let mut pane = Self {
            on_parent: state.cursor == 0,
            parent_asked: false,
            summary: Summary::of(&state),
            state,
            top: 0,
            page_rows: 1,
            columns: 1,
            width: 0.0,
            error: None,
            follow_notice: None,
        };
        pane.ensure_visible();
        pane
    }

    /// The engine's new state. After entering a directory the cursor stands on `..`, after going
    /// up on the directory left (where the engine put it); the view starts at the top and the
    /// old error goes. A cursor the engine moved off the first entry leaves the `..` row, and so
    /// does one it put on the first entry ([`Pane::show_cursor`]).
    pub fn show(&mut self, state: PanelState, navigated: bool) {
        if navigated && state.path != self.state.path {
            let went_up = self.state.path.parent().as_ref() == Some(&state.path);
            // A tab shown again keeps its cursor (RC-010-03); leaving a flat view (Ctrl+B) puts
            // the engine's cursor on a file at once.
            self.on_parent = !went_up && state.cursor == 0;
            self.parent_asked = false;
            self.top = 0;
            self.error = None;
        } else if state.cursor != 0 {
            self.on_parent = false;
        }
        let listed = Arc::ptr_eq(&self.state.entries, &state.entries)
            && self.state.dir_sizes == state.dir_sizes;
        if !listed {
            self.summary = Summary::of(&state);
        } else if self.state.selection != state.selection {
            // Only the selection changed: only the names that changed count (RC-080-53).
            self.summary = self
                .summary
                .reselected(&self.state, &state)
                .unwrap_or_else(|| Summary::of(&state));
        }
        self.state = state;
        self.ensure_visible();
    }

    /// The engine moved the cursor (`PanelChange::Cursor`): onto the first entry it leaves the
    /// `..` row - a `panel.set_cursor` from MCP or the engine means that entry - unless the window
    /// asked for `..` itself.
    pub fn show_cursor(&mut self, state: PanelState) {
        let asked = std::mem::take(&mut self.parent_asked);
        self.show(state, false);
        if self.state.cursor == 0 && self.on_parent != asked {
            self.on_parent = asked;
            self.ensure_visible();
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.state.entries
    }

    /// The engine's cursor: an index into the entries.
    pub fn cursor(&self) -> usize {
        self.state.cursor
    }

    /// Whether the panel shows the brief view.
    pub fn brief(&self) -> bool {
        self.state.mode == ListMode::Brief
    }

    /// Whether the listing starts with a `..` row: everywhere but at a root (the root of an
    /// archive has one: it leads out of the archive).
    pub fn has_parent_row(&self) -> bool {
        self.state.path.parent().is_some()
    }

    /// Whether the cursor stands on the `..` row; always in a directory without entries.
    pub fn on_parent(&self) -> bool {
        self.has_parent_row() && (self.on_parent || self.state.entries.is_empty())
    }

    /// The rows drawn: the `..` row and the entries.
    pub fn rows(&self) -> usize {
        self.entries().len() + usize::from(self.has_parent_row())
    }

    /// The row the cursor is drawn on.
    pub fn cursor_row(&self) -> usize {
        if self.on_parent() {
            0
        } else {
            self.cursor() + usize::from(self.has_parent_row())
        }
    }

    /// The entry a row shows; `None` for the `..` row.
    pub fn entry_index(&self, row: usize) -> Option<usize> {
        row.checked_sub(usize::from(self.has_parent_row()))
    }

    pub fn top(&self) -> usize {
        self.top
    }

    pub fn page_rows(&self) -> usize {
        self.page_rows
    }

    /// The columns on screen: those that fit in the brief view, one in the full view.
    pub fn columns(&self) -> usize {
        if self.brief() { self.columns } else { 1 }
    }

    /// Whether the entry of `row` is selected; the `..` row never is.
    pub fn is_selected(&self, row: usize) -> bool {
        self.entry_index(row)
            .and_then(|index| self.state.entries.get(index))
            .is_some_and(|entry| self.state.selection.contains(&entry.name))
    }

    /// The entries an operation of the panel takes, as F5 takes them: the selection in the
    /// listing's order, else the entry under the cursor; none on the `..` row, whose engine
    /// cursor is on the first entry.
    pub fn chosen_names(&self) -> Vec<String> {
        let state = &self.state;
        if !state.selection.is_empty() {
            return state
                .entries
                .iter()
                .filter(|entry| state.selection.contains(&entry.name))
                .map(|entry| entry.name.clone())
                .collect();
        }
        if self.on_parent() {
            return Vec::new();
        }
        state
            .cursor_entry()
            .map(|entry| vec![entry.name.clone()])
            .unwrap_or_default()
    }

    /// The selected entries; the tests count them, the status line sums them (`file_list.rs`).
    #[cfg(test)]
    pub fn selected_count(&self) -> usize {
        self.state.selection.len()
    }

    /// Files, directories and bytes of the panel and of its selection.
    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    /// The rows on screen. In the full view one more than fits, so a partly visible last row is
    /// drawn too; in the brief view the columns that fit, filled from the top.
    pub fn visible_range(&self) -> Range<usize> {
        let shown = if self.brief() {
            self.page_rows * self.columns
        } else {
            self.page_rows + 1
        };
        let end = (self.top + shown).min(self.rows());
        self.top.min(end)..end
    }

    /// The number of rows that fit, from the list's measured height.
    pub fn set_page_rows(&mut self, rows: usize) {
        self.page_rows = rows.max(1);
        self.ensure_visible();
    }

    /// The list's measured width, which the name column and the brief view's columns share.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// The columns of the brief view that fit in the list's measured width.
    pub fn set_width(&mut self, width: f32) {
        self.width = width;
        self.columns = ((width / BRIEF_COLUMN_WIDTH).floor() as usize).max(1);
        self.ensure_visible();
    }

    /// The mouse wheel: moves the view by `lines`, not the cursor; the brief view by a column.
    pub fn scroll(&mut self, lines: isize) {
        if self.brief() {
            let top_column = (self.top / self.page_rows).saturating_add_signed(lines.signum());
            self.top = top_column.min(self.last_top_column()) * self.page_rows;
        } else {
            let max_top = self.rows().saturating_sub(self.page_rows);
            self.top = self.top.saturating_add_signed(lines).min(max_top);
        }
    }

    /// The furthest first row: the last page's in the full view, the last columns' in the brief
    /// view.
    pub fn max_top(&self) -> usize {
        if self.brief() {
            self.last_top_column() * self.page_rows
        } else {
            self.rows().saturating_sub(self.page_rows)
        }
    }

    /// The scroll bar (RC-010-23): the view from row `top`, not past the last page; the brief
    /// view from the start of the column `top` falls in. The cursor stays where it is.
    pub fn scroll_to(&mut self, top: usize) {
        let top = top.min(self.max_top());
        self.top = if self.brief() {
            top - top % self.page_rows
        } else {
            top
        };
    }

    /// A key's command as the `..` row and the brief view's columns change it; `None` when the
    /// step stays in the window (onto or off the `..` row) and the engine has nothing to do.
    pub fn through_parent_row(&mut self, command: Command) -> Option<Command> {
        if let Command::MoveCursor {
            panel,
            step: step @ (CursorStep::Left | CursorStep::Right),
        } = command
        {
            return self.column_step(panel, step);
        }
        if !self.has_parent_row() {
            return Some(command);
        }
        let on_parent = self.on_parent();
        match command {
            Command::OpenEntry { panel } | Command::OpenFile { panel, path: None } if on_parent => {
                Some(Command::Navigate {
                    panel,
                    path: "..".into(),
                })
            }
            Command::ToggleCursorSelection { .. } | Command::SelectAndSize { .. } if on_parent => {
                self.leave_parent();
                None
            }
            // The `..` row has no properties, size, link or comment (RC-020-06).
            Command::GetProperties { .. }
            | Command::DirSize { .. }
            | Command::CreateLink { .. }
            | Command::SetComment { .. }
            | Command::CopyNames { .. }
                if on_parent && self.state.selection.is_empty() =>
            {
                None
            }
            Command::SelectExtension { .. } if on_parent => None,
            // F3 and F4 on `..` open nothing (RC-010-07).
            Command::OpenExternal { path: None, .. } if on_parent => None,
            Command::MoveCursor { panel, step } => self.step(panel, step),
            Command::SelectRange { panel, step } => self.range(panel, step),
            other => Some(other),
        }
    }

    /// A cursor step across the `..` row.
    fn step(&mut self, panel: Option<PanelId>, step: CursorStep) -> Option<Command> {
        let on_parent = self.on_parent();
        let page = self.page_rows.saturating_sub(1).max(1);
        let to_top = match step {
            CursorStep::Up | CursorStep::First | CursorStep::PageUp if on_parent => return None,
            CursorStep::Down if on_parent => {
                self.leave_parent();
                return None;
            }
            CursorStep::Up if self.cursor() == 0 => {
                self.on_parent = true;
                self.ensure_visible();
                return None;
            }
            CursorStep::First => true,
            CursorStep::PageUp => self.cursor() < page,
            _ => false,
        };
        self.on_parent = to_top;
        self.parent_asked = to_top;
        let step = if to_top { CursorStep::First } else { step };
        Some(Command::MoveCursor { panel, step })
    }

    /// Shift with a cursor key across the `..` row: never selected, it is stepped off without a
    /// selection and reached after the first entry was taken.
    fn range(&mut self, panel: Option<PanelId>, step: CursorStep) -> Option<Command> {
        let page = self.page_rows.saturating_sub(1).max(1);
        let command = Some(Command::SelectRange { panel, step });
        if self.on_parent() {
            return match step {
                CursorStep::Down => {
                    self.leave_parent();
                    None
                }
                CursorStep::PageDown | CursorStep::Last => {
                    self.on_parent = false;
                    command
                }
                _ => None,
            };
        }
        self.on_parent = match step {
            CursorStep::Up => self.cursor() == 0,
            CursorStep::PageUp => self.cursor() < page,
            CursorStep::First => true,
            _ => false,
        };
        self.parent_asked = self.on_parent;
        command
    }

    /// Left and Right: a column back or on in the brief view, `..` counted as a row; nothing in
    /// the full view.
    fn column_step(&mut self, panel: Option<PanelId>, step: CursorStep) -> Option<Command> {
        if !self.brief() || self.state.entries.is_empty() {
            return None;
        }
        let row = self.cursor_row();
        let target = match step {
            CursorStep::Left => row.saturating_sub(self.page_rows),
            _ => (row + self.page_rows).min(self.rows() - 1),
        };
        if target == row {
            return None;
        }
        self.on_parent = self.has_parent_row() && target == 0;
        self.parent_asked = self.on_parent;
        Some(match self.entry_index(target) {
            Some(index) if !self.on_parent => Command::SetCursor {
                panel,
                target: CursorTarget::Index(index),
            },
            _ => Command::MoveCursor {
                panel,
                step: CursorStep::First,
            },
        })
    }

    /// A click on `row`: the cursor goes there. The entry to put the engine's cursor on, which
    /// is the first one for the `..` row; `None` in a directory without entries.
    pub fn point_at(&mut self, row: usize) -> Option<usize> {
        match self.entry_index(row) {
            Some(index) => {
                self.on_parent = false;
                self.parent_asked = false;
                Some(index)
            }
            None => {
                self.on_parent = true;
                // The engine's answer comes only when its cursor is set.
                self.parent_asked = !self.state.entries.is_empty();
                (!self.state.entries.is_empty()).then_some(0)
            }
        }
    }

    fn leave_parent(&mut self) {
        self.on_parent = false;
        self.ensure_visible();
    }

    /// The first column the brief view can start with and still fill the panel.
    fn last_top_column(&self) -> usize {
        self.rows()
            .div_ceil(self.page_rows)
            .saturating_sub(self.columns)
    }

    /// Scrolls just enough to show the cursor, and never past the last page; the brief view by
    /// whole columns.
    fn ensure_visible(&mut self) {
        let cursor = self.cursor_row();
        if self.brief() {
            let rows = self.page_rows;
            let column = cursor / rows;
            let mut top_column = self.top / rows;
            if column < top_column {
                top_column = column;
            } else if column >= top_column + self.columns {
                top_column = column + 1 - self.columns;
            }
            self.top = top_column.min(self.last_top_column()) * rows;
            return;
        }
        if cursor < self.top {
            self.top = cursor;
        } else if cursor >= self.top + self.page_rows {
            self.top = cursor + 1 - self.page_rows;
        }
        self.top = self.top.min(self.rows().saturating_sub(self.page_rows));
    }
}

/// The extension column: what follows the last dot of a file's name - never of a directory, and
/// not the leading dot of a hidden file such as `.bashrc`.
pub fn ext(entry: &Entry) -> &str {
    extension(entry)
}

/// The name column: a file's name without the extension shown next to it.
pub fn stem(entry: &Entry) -> &str {
    let ext = ext(entry);
    if ext.is_empty() {
        &entry.name
    } else {
        &entry.name[..entry.name.len() - ext.len() - 1]
    }
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
