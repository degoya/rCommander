//! The dialog's hit list: the hits as they arrive and the part on screen ([`ListWindow`]).
//!
//! A duplicate search (RC-100-98) fills it group by group: each group's files are hits that
//! remember their group, and the list shows a line before each group with its size and count, as
//! Total Commander's result list has them. The cursor may stand on such a line; it is no hit.

use rc_engine::{MAX_FLAT_ENTRIES, SearchHit, search::DuplicateGroup};

use crate::app::list_window::ListWindow;

/// The rows the list shows at once until it knows its height.
pub const ROWS: usize = 12;

/// A row of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row<'a> {
    Hit(&'a SearchHit),
    /// The line before a duplicate group: the bytes of each file and how many there are.
    Group {
        size: u64,
        count: usize,
    },
}

/// A row of a duplicate result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Group { size: u64, count: usize },
    Hit(usize),
}

#[derive(Debug, Clone)]
pub struct Results {
    hits: Vec<SearchHit>,
    /// The group of each hit of a duplicate result; empty for a plain search.
    groups: Vec<u32>,
    /// The rows of a duplicate result; `None` for a plain search, whose rows are its hits.
    lines: Option<Vec<Line>>,
    rows: ListWindow,
    /// More hits were found than the list keeps ([`MAX_FLAT_ENTRIES`], what a panel can be fed).
    pub truncated: bool,
}

impl Default for Results {
    fn default() -> Self {
        Self {
            hits: Vec::new(),
            groups: Vec::new(),
            lines: None,
            rows: ListWindow::new(ROWS),
            truncated: false,
        }
    }
}

impl Results {
    pub fn hits(&self) -> &[SearchHit] {
        &self.hits
    }

    /// The group of each hit, in the order of [`Results::hits`]; empty unless the list holds a
    /// duplicate result.
    pub fn group_numbers(&self) -> &[u32] {
        &self.groups
    }

    /// Whether the list holds a duplicate result.
    pub fn is_duplicates(&self) -> bool {
        self.lines.is_some()
    }

    /// How many duplicate groups the list holds.
    pub fn group_count(&self) -> usize {
        self.groups.last().map_or(0, |last| *last as usize + 1)
    }

    /// How many hits the list holds.
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    /// How many rows the list shows: the hits, and the lines of the groups.
    pub fn row_count(&self) -> usize {
        self.lines.as_ref().map_or(self.hits.len(), Vec::len)
    }

    pub fn cursor(&self) -> usize {
        self.rows.cursor()
    }

    /// The row at `index`.
    pub fn row(&self, index: usize) -> Option<Row<'_>> {
        match &self.lines {
            None => self.hits.get(index).map(Row::Hit),
            Some(lines) => match lines.get(index)? {
                Line::Group { size, count } => Some(Row::Group {
                    size: *size,
                    count: *count,
                }),
                Line::Hit(hit) => self.hits.get(*hit).map(Row::Hit),
            },
        }
    }

    /// The hit under the cursor; `None` on a group's line.
    pub fn current(&self) -> Option<&SearchHit> {
        match self.row(self.rows.cursor())? {
            Row::Hit(hit) => Some(hit),
            Row::Group { .. } => None,
        }
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

    /// Adds a duplicate group: its line and its files; `false` once the list is full and the
    /// group was dropped.
    pub fn push_group(&mut self, group: DuplicateGroup) -> bool {
        if self.hits.len() + group.files.len() > MAX_FLAT_ENTRIES {
            self.truncated = true;
            return false;
        }
        let number = u32::try_from(self.group_count()).unwrap_or(u32::MAX);
        let lines = self.lines.get_or_insert_with(Vec::new);
        lines.push(Line::Group {
            size: group.size,
            count: group.files.len(),
        });
        for file in group.files {
            lines.push(Line::Hit(self.hits.len()));
            self.groups.push(number);
            self.hits.push(SearchHit {
                path: file.path,
                entry: file.entry,
            });
        }
        true
    }

    /// The indexes of the rows on screen.
    pub fn visible(&self) -> std::ops::Range<usize> {
        self.rows.visible(self.row_count())
    }

    /// The cursor moves by `by` rows (clamped) and stays on screen.
    pub fn step(&mut self, by: isize) {
        self.rows.step(by, self.row_count());
    }

    /// The cursor goes to row `index`, when there is one.
    pub fn point_at(&mut self, index: usize) {
        self.rows.point_at(index, self.row_count());
    }

    /// No hits, for a new search; the list keeps its height.
    pub fn clear(&mut self) {
        self.hits = Vec::new();
        self.groups = Vec::new();
        self.lines = None;
        self.truncated = false;
        self.rows.reset();
    }

    /// The rows that fit the list's height.
    pub fn page(&self) -> usize {
        self.rows.page()
    }

    /// The list is `rows` high now; the view stays as full as it can, the cursor on it.
    pub fn set_page(&mut self, rows: usize) {
        self.rows.set_page(rows, self.row_count());
    }

    /// The wheel: the view moves by `rows`, the cursor stays.
    pub fn scroll(&mut self, rows: isize) {
        self.rows.scroll(rows, self.row_count());
    }
}

#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;
