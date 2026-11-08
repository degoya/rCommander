//! Why a command of the tabs, the folder history, the hotlist or the panels' lists (file filters,
//! selections and list files, RC-100-92) was refused.

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
    /// A tab's name too long or holding a control character (RC-100-92).
    #[error("a tab's name: {reason}")]
    TabTitleInvalid { reason: &'static str },
    /// A saved file filter's name or mask that does not fit, or one too many (RC-100-92).
    #[error("a saved file filter: {reason}")]
    SavedFilterInvalid { reason: &'static str },
    #[error("there is no saved file filter named {name}")]
    SavedFilterNotFound { name: String },
    /// A selection or list file larger than the limit, or with more lines (RC-100-92).
    #[error("a list file holds at most {max_bytes} bytes and {max_lines} lines")]
    ListFileTooLarge { max_bytes: u64, max_lines: usize },
    /// A selection or list file somewhere else than on this machine's disks.
    #[error("a selection or list file lies on this machine's disks")]
    ListFileNotLocal,
    /// The selection file is there already; over MCP it is never overwritten.
    #[error("the file exists already")]
    ListFileExists,
    /// A list file without one absolute path to show.
    #[error("the list file names no absolute path")]
    ListFileEmpty,
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
        "tab_title_invalid",
        "saved_filter_invalid",
        "saved_filter_not_found",
        "list_file_too_large",
        "list_file_not_local",
        "file_exists",
        "list_file_empty",
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
            Self::TabTitleInvalid { .. } => "tab_title_invalid",
            Self::SavedFilterInvalid { .. } => "saved_filter_invalid",
            Self::SavedFilterNotFound { .. } => "saved_filter_not_found",
            Self::ListFileTooLarge { .. } => "list_file_too_large",
            Self::ListFileNotLocal => "list_file_not_local",
            Self::ListFileExists => "file_exists",
            Self::ListFileEmpty => "list_file_empty",
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
