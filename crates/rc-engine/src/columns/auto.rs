//! The automatic switch of the column view (RC-100-90, Total Commander's "Auto switch mode"):
//! a panel entering a folder whose path matches a column set's mask shows that set, the first in
//! the person's order winning; where no mask matches, `columns.auto_default` says what the panel
//! shows, as Total Commander's `<default>` mode. A mode chosen by hand stays until the panel
//! enters a folder for which another set holds - leaving a folder does not end it.
//!
//! The masks are Total Commander's (`*` any run of characters, `?` one, case ignored), matched
//! against the location as the path line shows it - `D:\Photos`, `sftp://server/home`,
//! `C:\data\backup.zip\docs` - with and without a closing separator, so `D:\Photos\*` holds for
//! the folder itself as for everything below it.

use super::{ColumnSet, ColumnSetId, PanelColumns};
use crate::{
    ListMode, PanelState,
    mask::{lowercase, wildcard},
};

/// What a panel shows where no mask matches (`columns.auto_default`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AutoDefault {
    /// The mode stays as it is (the default, as Total Commander's `<default>` mode).
    #[default]
    Unchanged,
    /// The full view.
    Full,
    /// The brief view.
    Brief,
}

impl AutoDefault {
    /// The stored value: `unchanged`, `full` or `brief`; anything else is `unchanged`.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value {
            "full" => Self::Full,
            "brief" => Self::Brief,
            _ => Self::Unchanged,
        }
    }

    fn apply(self, mode: ListMode) -> ListMode {
        match self {
            Self::Unchanged => mode,
            Self::Full => ListMode::Full,
            Self::Brief => ListMode::Brief,
        }
    }
}

/// Whether the folder mask `mask` holds for the location `shown`.
#[must_use]
pub fn mask_matches(mask: &str, shown: &str) -> bool {
    let pattern: Vec<char> = lowercase(mask.trim()).collect();
    if pattern.is_empty() {
        return false;
    }
    let separator = if shown.contains('\\') { '\\' } else { '/' };
    let closed = (!shown.ends_with(separator)).then(|| format!("{shown}{separator}"));
    wildcard(&pattern, lowercase(shown))
        || closed.is_some_and(|closed| wildcard(&pattern, lowercase(&closed)))
}

/// The first set of `sets` (in the person's order) one of whose masks holds for `shown`.
#[must_use]
pub fn matching_set(sets: &[ColumnSet], shown: &str) -> Option<ColumnSetId> {
    sets.iter()
        .find(|set| set.spec.masks.iter().any(|mask| mask_matches(mask, shown)))
        .map(|set| set.id)
}

/// What decides a panel's mode in a folder it enters: read before the panel's lock is taken.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FolderRule {
    /// The sets there are; `None` when they could not be read (no database), which leaves a
    /// set's mode alone.
    pub(crate) known: Option<Vec<ColumnSetId>>,
    /// `columns.auto_switch`.
    pub(crate) enabled: bool,
    /// `columns.auto_default`.
    pub(crate) default: AutoDefault,
    /// The set whose mask holds for the folder.
    pub(crate) matched: Option<ColumnSetId>,
}

impl FolderRule {
    /// `panel`, just showing the folder, in the mode the rule gives it: a set deleted since
    /// shows the full view; with the switch on, a matching mask or the default decides.
    pub(crate) fn apply(&self, panel: &mut PanelState) {
        if let (ListMode::Custom { set }, Some(known)) = (panel.mode, &self.known)
            && !known.contains(&set)
        {
            panel.mode = ListMode::Full;
        }
        if !self.enabled {
            return;
        }
        let (mode, columns) = switched(panel.mode, &panel.columns, self.matched, self.default);
        panel.mode = mode;
        panel.columns = columns;
    }
}

/// The mode and column state of a panel in `mode` with `columns` that enters a folder for which
/// the set `matched` holds (or none), `default` where none does.
#[must_use]
pub(crate) fn switched(
    mode: ListMode,
    columns: &PanelColumns,
    matched: Option<ColumnSetId>,
    default: AutoDefault,
) -> (ListMode, PanelColumns) {
    let mut next = PanelColumns {
        rule: matched,
        ..columns.clone()
    };
    let mode = match matched {
        // The same set as in the folder before: a mode chosen by hand stays.
        Some(_) if columns.manual && columns.rule == matched => mode,
        Some(set) => {
            next.manual = false;
            ListMode::Custom { set }
        }
        // No set holds: a mode by hand stays, else the default.
        None if columns.manual => mode,
        None => default.apply(mode),
    };
    (mode, next)
}

#[cfg(test)]
#[path = "auto_tests.rs"]
mod tests;
