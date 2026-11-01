//! The folders the panels start in (moved out of `schema.rs` to keep it under 500 lines).

use super::{LEFT_START_KEY, RIGHT_START_KEY, SettingKind, SettingPage, SettingSpec};

pub(super) const LEFT_START: SettingSpec = SettingSpec {
    key: LEFT_START_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Path { default: "" },
    label: "setting.folders.left_start",
    help: "setting.folders.left_start.help",
    protected: false,
    restart: true,
};

pub(super) const RIGHT_START: SettingSpec = SettingSpec {
    key: RIGHT_START_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Path { default: "" },
    label: "setting.folders.right_start",
    help: "setting.folders.right_start.help",
    protected: false,
    restart: true,
};
