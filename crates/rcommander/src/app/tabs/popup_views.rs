//! The menu of the views a panel shows (RC-100-90, Total Commander's `cm_SrcCustomViewMenu`): a
//! right click on the column header opens it - the full and the brief view, the comments, every
//! column set in the person's order, and "Configure..." for the settings' Columns tab. The view
//! the panel shows is marked and holds the cursor.

use rc_engine::{ColumnSet, ListMode, PanelId};

use super::popup::{Popup, PopupAction, PopupRow, row};
use crate::i18n::t;

impl Popup {
    /// The views of `panel`, which shows `mode`, with the column sets `sets`.
    pub fn views(panel: PanelId, sets: &[ColumnSet], mode: ListMode) -> Self {
        let fixed = [
            (t!("columns.menu.full").to_owned(), ListMode::Full),
            (t!("columns.menu.brief").to_owned(), ListMode::Brief),
            (t!("columns.menu.comments").to_owned(), ListMode::Comments),
        ];
        let named = sets
            .iter()
            .map(|set| (set.name.clone(), ListMode::Custom { set: set.id }));
        let mut rows: Vec<PopupRow> = fixed
            .into_iter()
            .chain(named)
            .map(|(label, view)| PopupRow {
                current: view == mode,
                ..row(label, 0, Some(PopupAction::View(view)))
            })
            .collect();
        rows.push(row(
            t!("columns.menu.configure").to_owned(),
            0,
            Some(PopupAction::ConfigureColumns),
        ));
        let cursor = rows.iter().position(|row| row.current).unwrap_or(0);
        Self {
            panel,
            rows,
            cursor,
        }
    }
}

#[cfg(test)]
#[path = "popup_views_tests.rs"]
mod tests;
