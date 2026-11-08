//! The history list as Total Commander 11 shows it (RC-100-92): Alt+Down lists the tab's folders,
//! newest first, and below them the most visited folders with their visit counts; typing filters
//! both by part of the path (`*` and `?` as the quick filter takes them), Backspace takes the
//! last character back, and Escape empties the search before it closes the list. Pure data, so
//! the keys are tested without a window.

use rc_engine::{Chord, FrequentFolder, quick_search::NameMatch, shown_path};

use super::popup::{Popup, PopupAction, PopupKey, PopupRow, row};
use crate::{app::menu_letter, i18n::t};

/// The search over an open history list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySearch {
    /// Every row of the list as it opened.
    all: Vec<PopupRow>,
    /// What was typed.
    pub typed: String,
}

/// The history `popup` with the most visited folders `frequent` below a heading, and its search.
pub fn with_frequent(mut popup: Popup, frequent: &[FrequentFolder]) -> (Popup, HistorySearch) {
    if !frequent.is_empty() {
        popup
            .rows
            .push(row(t!("history.frequent").to_owned(), 0, None));
        popup
            .rows
            .extend(frequent.iter().enumerate().map(|(index, folder)| PopupRow {
                detail: folder.visits.to_string(),
                ..row(
                    shown_path(&folder.path).display().to_string(),
                    0,
                    Some(PopupAction::OpenFrequent(index)),
                )
            }));
    }
    let search = HistorySearch {
        all: popup.rows.clone(),
        typed: String::new(),
    };
    (popup, search)
}

impl HistorySearch {
    /// Whether `popup` is the list this search belongs to: every row it shows is one of it.
    pub fn owns(&self, popup: &Popup) -> bool {
        popup.rows.iter().all(|row| self.all.contains(row))
    }

    /// What `chord` does to the search, which changes `popup`'s rows; `None` for a key the list
    /// itself handles.
    pub fn key(&mut self, chord: &Chord, popup: &mut Popup) -> Option<PopupKey> {
        if menu_letter::is_letter(chord) {
            self.typed.push_str(&chord.key);
        } else if chord.is_plain() && chord.key == "Backspace" {
            self.typed.pop()?;
        } else if chord.is_plain() && chord.key == "Escape" && !self.typed.is_empty() {
            self.typed.clear();
        } else {
            return None;
        }
        self.apply(popup);
        Some(PopupKey::Stay)
    }

    /// `popup` shows the rows the typed text matches, a heading with a matching row after it;
    /// the cursor on the first one.
    fn apply(&self, popup: &mut Popup) {
        let matcher = NameMatch::filter(&self.typed);
        let matches = |row: &PopupRow| {
            matcher
                .as_ref()
                .is_none_or(|matcher| matcher.matches(&row.label))
        };
        let mut rows: Vec<PopupRow> = Vec::new();
        let mut heading: Option<&PopupRow> = None;
        for row in &self.all {
            if row.action.is_none() {
                heading = Some(row);
                continue;
            }
            if matches(row) {
                if let Some(heading) = heading.take() {
                    rows.push(heading.clone());
                }
                rows.push(row.clone());
            }
        }
        popup.cursor = rows
            .iter()
            .position(|row| row.current && matcher.is_none())
            .or_else(|| rows.iter().position(|row| row.action.is_some()))
            .unwrap_or(0);
        popup.rows = rows;
    }
}

#[cfg(test)]
#[path = "history_search_tests.rs"]
mod tests;
