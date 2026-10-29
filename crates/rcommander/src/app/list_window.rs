//! The part of a long list on screen, shared by find files' hits, the synchronize-dirs rows,
//! the diff's lines and the multi-rename table: a cursor, the first row shown and how many rows
//! fit. Only those rows are ever drawn, so a hundred thousand cost what a dozen do. The wheel
//! moves the view by the whole rows `theme::wheel::rows` makes of it (RC-080-44), the cursor
//! stays; a step of the cursor brings the view along. The list itself is its owner's: each call
//! names its length.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListWindow {
    cursor: usize,
    top: usize,
    /// The rows that fit the list's height.
    page: usize,
}

impl ListWindow {
    /// `page` rows until the list knows its height.
    pub fn new(page: usize) -> Self {
        Self {
            cursor: 0,
            top: 0,
            page: page.max(1),
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The first row on screen.
    pub fn top(&self) -> usize {
        self.top
    }

    /// The rows that fit the list's height.
    pub fn page(&self) -> usize {
        self.page
    }

    /// The indexes of the rows on screen of a list `len` long.
    pub fn visible(&self, len: usize) -> Range<usize> {
        self.top..(self.top + self.page).min(len)
    }

    /// The cursor moves by `by` rows (clamped to the list) and stays on screen.
    pub fn step(&mut self, by: isize, len: usize) {
        let last = len.saturating_sub(1);
        self.cursor = self.cursor.saturating_add_signed(by).min(last);
        self.follow();
    }

    /// The cursor goes to row `index`, when there is one.
    pub fn point_at(&mut self, index: usize, len: usize) {
        if index < len {
            self.cursor = index;
            self.follow();
        }
    }

    /// The cursor is on row `index` now - the list changed under it - and stays on screen.
    pub fn set_cursor(&mut self, index: usize) {
        self.cursor = index;
        self.follow();
    }

    /// The list is `rows` high now; the view stays as full as it can, the cursor on it.
    pub fn set_page(&mut self, rows: usize, len: usize) {
        self.fit(rows, len);
        self.follow();
    }

    /// [`ListWindow::set_page`] for a list without a cursor: only the view is kept full.
    pub fn fit(&mut self, rows: usize, len: usize) {
        self.page = rows.max(1);
        self.set_top(self.top, len);
    }

    /// The wheel: the view moves by `rows`, never past the end; the cursor stays.
    pub fn scroll(&mut self, rows: isize, len: usize) {
        self.set_top(self.top.saturating_add_signed(rows), len);
    }

    /// The view starts at row `top`, or as near as the end allows; the cursor stays.
    pub fn set_top(&mut self, top: usize, len: usize) {
        self.top = top.min(len.saturating_sub(self.page));
    }

    /// Back to the first row, cursor and view, for a new list; the height stays.
    pub fn reset(&mut self) {
        self.cursor = 0;
        self.top = 0;
    }

    fn follow(&mut self) {
        if self.cursor < self.top {
            self.top = self.cursor;
        } else if self.cursor >= self.top + self.page {
            self.top = self.cursor + 1 - self.page;
        }
    }
}

#[cfg(test)]
#[path = "list_window_tests.rs"]
mod tests;
