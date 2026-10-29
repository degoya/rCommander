//! How names sort (owner, 2026-10-19): Windows' word sort as Total Commander shows it, the byte
//! order of Linux' `ls` in the C locale, or each by the panel's target - Windows' on the drives
//! of this Windows machine, Linux' on SSH, FTP and WSL. Numbers by their value (`file2` before
//! `file10`) is a switch of its own, on by default as in Total Commander. Neither is protected.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// Which order names follow: `windows`, `linux` or `by_target`.
pub const SORT_NAMES_KEY: &str = "sort.names";
/// Whether runs of digits in names compare by their value.
pub const SORT_NUMBERS_KEY: &str = "sort.numbers_by_value";

const ORDERS: &[Choice] = &[
    Choice {
        value: "windows",
        label: "setting.sort.names.windows",
    },
    Choice {
        value: "linux",
        label: "setting.sort.names.linux",
    },
    Choice {
        value: "by_target",
        label: "setting.sort.names.by_target",
    },
];

pub(super) const SORT_NAMES: SettingSpec = SettingSpec {
    key: SORT_NAMES_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Choice {
        choices: ORDERS,
        default: "windows",
    },
    label: "setting.sort.names",
    help: "setting.sort.names.help",
    protected: false,
    restart: false,
};

pub(super) const SORT_NUMBERS: SettingSpec = SettingSpec {
    key: SORT_NUMBERS_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Bool { default: true },
    label: "setting.sort.numbers_by_value",
    help: "setting.sort.numbers_by_value.help",
    protected: false,
    restart: false,
};
