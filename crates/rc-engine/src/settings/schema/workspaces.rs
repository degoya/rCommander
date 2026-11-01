//! The workspaces' settings (RC-100-68), on the Folders page beside the start folders: what
//! applying a workspace does to the tabs a panel has, and the workspace the window starts with.
//! Neither is protected.

use super::{Choice, SettingKind, SettingPage, SettingSpec};
use crate::workspaces::{APPLY_MODE_KEY, ON_START_KEY};

const APPLY_MODES: &[Choice] = &[
    Choice {
        value: "replace_keep_locked",
        label: "setting.workspaces.apply_mode.replace_keep_locked",
    },
    Choice {
        value: "replace_all",
        label: "setting.workspaces.apply_mode.replace_all",
    },
    Choice {
        value: "add",
        label: "setting.workspaces.apply_mode.add",
    },
];

/// The workspace's tabs replace the panel's, its locked tabs stay (owner, 2026-11-01).
pub(super) const APPLY_MODE: SettingSpec = SettingSpec {
    key: APPLY_MODE_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Choice {
        choices: APPLY_MODES,
        default: "replace_keep_locked",
    },
    label: "setting.workspaces.apply_mode",
    help: "setting.workspaces.apply_mode.help",
    protected: false,
    restart: false,
};

/// A workspace's id, or empty for the tabs of the last run; read at the next start.
pub(super) const ON_START: SettingSpec = SettingSpec {
    key: ON_START_KEY,
    page: SettingPage::Folders,
    kind: SettingKind::Id { default: "" },
    label: "setting.workspaces.on_start",
    help: "setting.workspaces.on_start.help",
    protected: false,
    restart: true,
};
