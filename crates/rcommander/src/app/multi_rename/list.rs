//! The preview table: the entries in the order the counter counts them, the latest preview of
//! each, and the part on screen ([`ListWindow`]). The order is the panel's at first; sorting by
//! name and moving a row change it, and with it the counter values.

use rc_engine::{Change, RenameItem};

use crate::app::list_window::ListWindow;

/// The rows the table shows at once until it knows its height.
pub const ROWS: usize = 14;

#[derive(Debug, Clone)]
pub struct RenameList {
    /// The entries to rename, in the counter's order.
    names: Vec<String>,
    /// The preview of each entry, in the same order; empty until the first preview arrives.
    items: Vec<RenameItem>,
    rows: ListWindow,
    /// Sorted by name, which way: the next click turns it around.
    descending: bool,
}

impl Default for RenameList {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            items: Vec::new(),
            rows: ListWindow::new(ROWS),
            descending: false,
        }
    }
}

impl RenameList {
    pub fn new(names: Vec<String>) -> Self {
        Self {
            names,
            ..Self::default()
        }
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn cursor(&self) -> usize {
        self.rows.cursor()
    }

    /// The entries get other names (after a run, or its undo); the preview is gone until the
    /// next one arrives.
    pub fn set_names(&mut self, names: Vec<String>) {
        self.names = names;
        self.items.clear();
        let last = self.names.len().saturating_sub(1);
        self.rows.set_cursor(self.rows.cursor().min(last));
    }

    /// The preview of the names as they are; a preview of another order is not shown.
    pub fn set_items(&mut self, items: Vec<RenameItem>) {
        let fits = items.len() == self.names.len()
            && items
                .iter()
                .zip(&self.names)
                .all(|(item, name)| item.old == *name);
        self.items = if fits { items } else { Vec::new() };
    }

    /// Row `index`: the old name and, once previewed, its line.
    pub fn row(&self, index: usize) -> Option<(&str, Option<&RenameItem>)> {
        let name = self.names.get(index)?;
        Some((name.as_str(), self.items.get(index)))
    }

    /// The pairs `old -> new` the preview would rename.
    pub fn changes(&self) -> Vec<(String, String)> {
        self.items
            .iter()
            .filter(|item| item.change != Change::Unchanged)
            .map(|item| (item.old.clone(), item.new.clone()))
            .collect()
    }

    /// The indexes of the rows on screen.
    pub fn visible(&self) -> std::ops::Range<usize> {
        self.rows.visible(self.names.len())
    }

    /// The cursor moves by `by` rows (clamped) and stays on screen.
    pub fn step(&mut self, by: isize) {
        self.rows.step(by, self.names.len());
    }

    pub fn point_at(&mut self, index: usize) {
        self.rows.point_at(index, self.names.len());
    }

    /// The rows that fit the table's height.
    pub fn page(&self) -> usize {
        self.rows.page()
    }

    /// The table is `rows` high now; the view stays as full as it can, the cursor on it.
    pub fn set_page(&mut self, rows: usize) {
        self.rows.set_page(rows, self.names.len());
    }

    /// The wheel: the view moves by `rows`, the cursor stays.
    pub fn scroll(&mut self, rows: isize) {
        self.rows.scroll(rows, self.names.len());
    }

    /// The row under the cursor moves one up (`-1`) or down (`1`), the cursor with it; `false`
    /// when it is at that end already.
    pub fn move_row(&mut self, by: isize) -> bool {
        let cursor = self.rows.cursor();
        let Some(to) = cursor.checked_add_signed(by) else {
            return false;
        };
        if to >= self.names.len() {
            return false;
        }
        self.names.swap(cursor, to);
        self.items.clear();
        self.rows.set_cursor(to);
        true
    }

    /// Sorted by name, without regard to case, the other way round than the last time; the
    /// cursor stays on its entry.
    pub fn sort_by_name(&mut self) {
        let current = self.names.get(self.rows.cursor()).cloned();
        self.names.sort_by_cached_key(|name| name.to_lowercase());
        if self.descending {
            self.names.reverse();
        }
        self.descending = !self.descending;
        self.items.clear();
        let index = current.and_then(|name| self.names.iter().position(|n| *n == name));
        let cursor = index.unwrap_or(self.rows.cursor());
        self.rows.set_cursor(cursor);
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
