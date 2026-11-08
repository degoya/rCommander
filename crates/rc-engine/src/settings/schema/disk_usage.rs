//! The disk-usage window (Ctrl+Shift+L, RC-100-98), shown on the page Folders: whether it shows and
//! sorts by the bytes of the files or by what they occupy on the volume, and how many of the
//! largest files it lists.

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// `bytes` (the files' lengths) or `allocated` (what they occupy on the volume, where the backend
/// tells it).
pub const DISK_USAGE_SIZE_KEY: &str = "disk_usage.size";
/// How many of the largest files the window lists.
pub const DISK_USAGE_LARGEST_KEY: &str = "disk_usage.largest_files";

const SIZES: &[Choice] = &[
    Choice {
        value: "bytes",
        label: "setting.disk_usage.size.bytes",
    },
    Choice {
        value: "allocated",
        label: "setting.disk_usage.size.allocated",
    },
];

pub(super) const SIZE: SettingSpec = SettingSpec {
    key: DISK_USAGE_SIZE_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Choice {
        choices: SIZES,
        default: "bytes",
    },
    label: "setting.disk_usage.size",
    help: "setting.disk_usage.size.help",
    protected: false,
    restart: false,
};

pub(super) const LARGEST: SettingSpec = SettingSpec {
    key: DISK_USAGE_LARGEST_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Int {
        min: 10,
        max: 1000,
        default: 100,
    },
    label: "setting.disk_usage.largest_files",
    help: "setting.disk_usage.largest_files.help",
    protected: false,
    restart: false,
};
