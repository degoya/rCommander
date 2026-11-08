//! The column sets in the command palette (RC-100-90): one row per set, "Column set: Photos",
//! made from the sets the window knows (read at the start and after every change,
//! `app/resize/sets.rs`) each time the palette opens. A row shows the set in the active panel
//! (`panel.set_view` with the set, chosen by hand); the row of the set the active panel shows is
//! marked. A key of its own per set comes later (the job file).

use rc_engine::{ColumnSet, Command, ListMode};

use super::{Action, Entry};
use crate::i18n::t;

/// The name of a set's row: `column_set:<id>`, which no `keys.` key has.
fn row_name(id: i64) -> String {
    format!("column_set:{id}")
}

/// "Column set: Photos".
pub fn row_label(name: &str) -> String {
    t!("palette.column_set", name = name)
}

/// `sets` as rows of the palette among `entries`, all sorted by label again; `shown` is the
/// active panel's view.
pub fn add_rows(entries: &mut Vec<Entry>, sets: &[ColumnSet], shown: ListMode) {
    for set in sets {
        let mode = ListMode::Custom { set: set.id };
        entries.push(Entry {
            name: row_name(set.id),
            label: row_label(&set.name),
            keys: String::new(),
            chords: String::new(),
            action: Action::Run(Box::new(Command::SetListMode { panel: None, mode })),
            current: shown == mode,
        });
    }
    entries.sort_by_key(|entry| entry.label.to_lowercase());
}

#[cfg(test)]
#[path = "column_sets_tests.rs"]
mod tests;
