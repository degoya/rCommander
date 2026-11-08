//! The settings of the panels' smaller Total Commander functions (RC-100-92), on the page
//! Folders: the programs of Ctrl+F11, whether each tab keeps its own file filter, whether closing
//! all tabs asks, the ignore list and its switch, and how many of the most visited folders the
//! history list shows. None is protected.

use super::{SettingKind, SettingPage, SettingSpec};
use crate::{
    file_filter::{DEFAULT_PROGRAMS, FILTER_PER_TAB_KEY, PROGRAMS_KEY},
    ignore_list::{IGNORE_ENABLED_KEY, IGNORE_ENTRIES_KEY},
    tabs::{CONFIRM_CLOSE_ALL_KEY, FREQUENT_SHOWN_KEY},
};

/// Total Commander's programs (`*.exe;*.com;*.bat;*.pif;*.cmd`); off Windows a file with an
/// execute bit counts too.
pub(super) const PROGRAMS: SettingSpec = SettingSpec {
    key: PROGRAMS_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Extensions {
        default: DEFAULT_PROGRAMS,
    },
    label: "setting.filters.programs",
    help: "setting.filters.programs.help",
    protected: false,
    restart: false,
};

/// Total Commander's "Save displayed file types separately for each tab" (`DirTabFilters`), on
/// by default as there.
pub(super) const FILTER_PER_TAB: SettingSpec = SettingSpec {
    key: FILTER_PER_TAB_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.tabs.filter_per_tab",
    help: "setting.tabs.filter_per_tab.help",
    protected: false,
    restart: false,
};

pub(super) const CONFIRM_CLOSE_ALL: SettingSpec = SettingSpec {
    key: CONFIRM_CLOSE_ALL_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.tabs.confirm_close_all",
    help: "setting.tabs.confirm_close_all.help",
    protected: false,
    restart: false,
};

/// Off by default, as Total Commander's `IgnoreListFileEnabled`.
pub(super) const IGNORE_ENABLED: SettingSpec = SettingSpec {
    key: IGNORE_ENABLED_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: false },
    label: "setting.ignore_list.enabled",
    help: "setting.ignore_list.enabled.help",
    protected: false,
    restart: false,
};

pub(super) const IGNORE_ENTRIES: SettingSpec = SettingSpec {
    key: IGNORE_ENTRIES_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::MaskLines { default: "" },
    label: "setting.ignore_list.entries",
    help: "setting.ignore_list.entries.help",
    protected: false,
    restart: false,
};

/// Total Commander 11's frequently used folders under the history (Alt+Down); 0 counts nothing.
pub(super) const FREQUENT_SHOWN: SettingSpec = SettingSpec {
    key: FREQUENT_SHOWN_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Int {
        min: 0,
        max: 50,
        default: 10,
    },
    label: "setting.history.frequent_shown",
    help: "setting.history.frequent_shown.help",
    protected: false,
    restart: false,
};
