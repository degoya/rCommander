//! The find-files dialog (Alt+F7, RC-100-93), shown on the page Folders: whether "Search
//! archives" starts ticked, and whether the dialog offers Everything where it runs.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether "Search archives" is ticked when the dialog opens for the first time.
pub const SEARCH_ARCHIVES_DEFAULT_KEY: &str = "search.archives_default";
/// Whether the dialog offers "Everything" when Everything runs (Windows).
pub const SEARCH_EVERYTHING_KEY: &str = "search.everything";

pub(super) const ARCHIVES_DEFAULT: SettingSpec = SettingSpec {
    key: SEARCH_ARCHIVES_DEFAULT_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.search.archives_default",
    help: "setting.search.archives_default.help",
    protected: false,
    restart: false,
};

pub(super) const EVERYTHING: SettingSpec = SettingSpec {
    key: SEARCH_EVERYTHING_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.search.everything",
    help: "setting.search.everything.help",
    protected: false,
    restart: false,
};
