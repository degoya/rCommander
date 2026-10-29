//! The dialog's hit list: the hits as they arrive and the part on screen ([`ListWindow`]).

use rc_engine::{MAX_FLAT_ENTRIES, SearchHit};

use crate::app::list_window::ListWindow;

/// The rows the list shows at once until it knows its height.
pub const ROWS: usize = 12;

#[derive(Debug, Clone)]
pub struct Results {
    hits: Vec<SearchHit>,
    rows: ListWindow,
    /// More hits were found than the list keeps ([`MAX_FLAT_ENTRIES`], what a panel can be fed).
    pub truncated: bool,
}

impl Default for Results {
    fn default() -> Self {
        Self {
            hits: Vec::new(),
            rows: ListWindow::new(ROWS),
            truncated: false,
        }
    }
}

impl Results {
    pub fn hits(&self) -> &[SearchHit] {
        &self.hits
    }

    pub fn len(&self) -> usize {
        self.hits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.rows.cursor()
    }

    /// The hit under the cursor.
    pub fn current(&self) -> Option<&SearchHit> {
        self.hits.get(self.rows.cursor())
    }

    /// Adds a hit; `false` once the list is full and the hit was dropped.
    pub fn push(&mut self, hit: SearchHit) -> bool {
        if self.hits.len() >= MAX_FLAT_ENTRIES {
            self.truncated = true;
            return false;
        }
        self.hits.push(hit);
        true
    }

    /// The indexes of the hits on screen.
    pub fn visible(&self) -> std::ops::Range<usize> {
        self.rows.visible(self.hits.len())
    }

    /// The cursor moves by `by` hits (clamped) and stays on screen.
    pub fn step(&mut self, by: isize) {
        self.rows.step(by, self.hits.len());
    }

    /// The cursor goes to hit `index`, when there is one.
    pub fn point_at(&mut self, index: usize) {
        self.rows.point_at(index, self.hits.len());
    }

    /// No hits, for a new search; the list keeps its height.
    pub fn clear(&mut self) {
        self.hits = Vec::new();
        self.truncated = false;
        self.rows.reset();
    }

    /// The rows that fit the list's height.
    pub fn page(&self) -> usize {
        self.rows.page()
    }

    /// The list is `rows` high now; the view stays as full as it can, the cursor on it.
    pub fn set_page(&mut self, rows: usize) {
        self.rows.set_page(rows, self.hits.len());
    }

    /// The wheel: the view moves by `rows`, the cursor stays.
    pub fn scroll(&mut self, rows: isize) {
        self.rows.scroll(rows, self.hits.len());
    }
}

#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;
