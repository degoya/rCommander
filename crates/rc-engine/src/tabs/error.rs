//! Why a command of the tabs, the folder history or the hotlist was refused.

use rc_db::HotlistError;

use super::TabId;
use crate::EngineError;

/// A refusal of a `tabs.*`, `history.*` or `hotlist.*` command, each with a stable code.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TabsError {
    /// No tab of the panel has this id.
    #[error("the panel has no tab {}", tab.0)]
    TabNotFound { tab: TabId },
    /// A panel keeps at least one tab.
    #[error("the last tab of a panel stays open")]
    LastTab,
    /// Nothing lies back or ahead in the tab's folder history.
    #[error("the folder history has nothing further in this direction")]
    HistoryEnd,
    /// A history index past the tab's history.
    #[error("index {index} is outside the folder history's {len} entries")]
    HistoryOutOfRange { index: usize, len: usize },
    #[error("there is no hotlist entry {id}")]
    HotlistNotFound { id: i64 },
    #[error("hotlist entry {id} is not a folder")]
    HotlistNotAFolder { id: i64 },
    /// A path or a target given for a hotlist folder, or a folder opened like an entry.
    #[error("hotlist entry {id} is a folder; it has no path")]
    HotlistNotAnEntry { id: i64 },
    #[error("a hotlist folder cannot move into itself or one of its folders")]
    HotlistCycle,
    #[error("the hotlist nests at most {max} levels deep")]
    HotlistTooDeep { max: usize },
    #[error("a hotlist name is not blank and has at most {max} characters")]
    HotlistBadName { max: usize },
    /// A folder whose name cannot be stored (not Unicode, or too long), or a stored entry that
    /// does not read.
    #[error("this folder cannot be kept in the hotlist")]
    HotlistBadPath,
}

impl TabsError {
    /// Every code of these refusals.
    pub const ALL_CODES: &[&str] = &[
        "tab_not_found",
        "last_tab",
        "history_end",
        "history_out_of_range",
        "hotlist_not_found",
        "hotlist_not_a_folder",
        "hotlist_not_an_entry",
        "hotlist_cycle",
        "hotlist_too_deep",
        "hotlist_bad_name",
        "hotlist_bad_path",
    ];

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::TabNotFound { .. } => "tab_not_found",
            Self::LastTab => "last_tab",
            Self::HistoryEnd => "history_end",
            Self::HistoryOutOfRange { .. } => "history_out_of_range",
            Self::HotlistNotFound { .. } => "hotlist_not_found",
            Self::HotlistNotAFolder { .. } => "hotlist_not_a_folder",
            Self::HotlistNotAnEntry { .. } => "hotlist_not_an_entry",
            Self::HotlistCycle => "hotlist_cycle",
            Self::HotlistTooDeep { .. } => "hotlist_too_deep",
            Self::HotlistBadName { .. } => "hotlist_bad_name",
            Self::HotlistBadPath => "hotlist_bad_path",
        }
    }
}

impl From<HotlistError> for EngineError {
    fn from(error: HotlistError) -> Self {
        let refusal = match error {
            HotlistError::NotFound { id } => TabsError::HotlistNotFound { id },
            HotlistError::NotAFolder { id } => TabsError::HotlistNotAFolder { id },
            HotlistError::NotAnEntry { id } => TabsError::HotlistNotAnEntry { id },
            HotlistError::Cycle => TabsError::HotlistCycle,
            HotlistError::TooDeep { max } => TabsError::HotlistTooDeep { max },
            HotlistError::BadName { max } => TabsError::HotlistBadName { max },
            HotlistError::Db(error) => return Self::Database(error),
        };
        Self::Tabs(refusal)
    }
}

impl From<TabsError> for EngineError {
    fn from(error: TabsError) -> Self {
        Self::Tabs(error)
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
