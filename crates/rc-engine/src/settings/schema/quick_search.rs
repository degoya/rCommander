//! The quick search of the file lists, shown on the page Folders: which keys start it, whether
//! it matches anywhere in a name, tells upper from lower case, and hides what does not match.

use super::{SettingKind, SettingPage, SettingSpec, choices};

/// Which keys start the quick search: Ctrl+Alt with letters, Alt with letters, or letters alone.
pub const QUICK_SEARCH_KEYS_KEY: &str = "quick_search.keys";
/// Whether the quick search matches anywhere in a name instead of at its start.
pub const QUICK_SEARCH_ANYWHERE_KEY: &str = "quick_search.anywhere";
/// Whether the quick search tells upper from lower case.
pub const QUICK_SEARCH_CASE_KEY: &str = "quick_search.case_sensitive";
/// Whether the quick search hides every entry that does not match, as the quick filter does,
/// instead of only moving the cursor (owner, 2026-10-19).
pub const QUICK_SEARCH_FILTERS_KEY: &str = "quick_search.filters";

pub(super) const KEYS: SettingSpec = SettingSpec {
    key: QUICK_SEARCH_KEYS_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Choice {
        choices: choices::QUICK_SEARCH_KEYS,
        default: "ctrl_alt",
    },
    label: "setting.quick_search.keys",
    help: "setting.quick_search.keys.help",
    protected: false,
    restart: false,
};

pub(super) const ANYWHERE: SettingSpec = SettingSpec {
    key: QUICK_SEARCH_ANYWHERE_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.quick_search.anywhere",
    help: "setting.quick_search.anywhere.help",
    protected: false,
    restart: false,
};

pub(super) const CASE_SENSITIVE: SettingSpec = SettingSpec {
    key: QUICK_SEARCH_CASE_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.quick_search.case_sensitive",
    help: "setting.quick_search.case_sensitive.help",
    protected: false,
    restart: false,
};

pub(super) const FILTERS: SettingSpec = SettingSpec {
    key: QUICK_SEARCH_FILTERS_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.quick_search.filters",
    help: "setting.quick_search.filters.help",
    protected: false,
    restart: false,
};
